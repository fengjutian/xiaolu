param(
    [string]$Configuration = "release",
    [string]$Output = "tools-dist"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$toolsRoot = Join-Path $root "tools"
$outRoot = Join-Path $root $Output

New-Item -ItemType Directory -Force -Path $outRoot | Out-Null

Get-ChildItem -Path $toolsRoot -Directory | Where-Object { $_.Name -notlike "_*" } | ForEach-Object {
    $tool = $_
    $manifestPath = Join-Path $tool.FullName "tool.json"
    if (-not (Test-Path $manifestPath)) {
        Write-Warning "Skip $($tool.Name): no tool.json"
        return
    }
    $manifestText = [IO.File]::ReadAllText($manifestPath, [Text.Encoding]::UTF8)
    $manifest = $manifestText | ConvertFrom-Json
    $project = Join-Path $tool.FullName "Cargo.toml"
    if (-not (Test-Path $project)) {
        Write-Warning "Skip $($tool.Name): no Cargo.toml"
        return
    }
    Write-Host "Building $($tool.Name)..."
    cargo build "--$Configuration" --manifest-path $project
    if ($LASTEXITCODE -ne 0) { throw "Build failed for $($tool.Name)" }

    $targetDir = if ($Configuration -eq "release") { "release" } else { "debug" }
    $binary = Join-Path $tool.FullName "target/$targetDir/$($manifest.entry)"
    if (-not (Test-Path $binary)) { throw "Missing binary: $binary" }

    $destination = Join-Path $outRoot $manifest.id
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    Copy-Item $binary $destination -Force
    Copy-Item $manifestPath $destination -Force
    Copy-Item (Join-Path $tool.FullName "README.md") $destination -Force -ErrorAction SilentlyContinue
    Write-Host "  -> $destination"
}

Write-Host "Tool packaging complete: $outRoot"
