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

# lens_helper 不是工具箱协议工具，单独构建并放到 software_pm 同目录。
$helperRoot = Join-Path $toolsRoot "lens_helper"
$helperProject = Join-Path $helperRoot "Cargo.toml"
if (Test-Path $helperProject) {
    Write-Host "Building lens_helper..."
    cargo build "--$Configuration" --manifest-path $helperProject
    if ($LASTEXITCODE -ne 0) { throw "Build failed for lens_helper" }
    $targetDir = if ($Configuration -eq "release") { "release" } else { "debug" }
    $helperBinary = Join-Path $helperRoot "target/$targetDir/lens_helper.exe"
    if (-not (Test-Path $helperBinary)) { throw "Missing binary: $helperBinary" }
    $softwarePmDir = Join-Path $outRoot "software_pm"
    if (-not (Test-Path $softwarePmDir)) {
        New-Item -ItemType Directory -Force -Path $softwarePmDir | Out-Null
    }
    Copy-Item $helperBinary (Join-Path $softwarePmDir "lens_helper.exe") -Force
    $aiCoolingDir = Join-Path $outRoot "ai_cooling"
    if (-not (Test-Path $aiCoolingDir)) {
        New-Item -ItemType Directory -Force -Path $aiCoolingDir | Out-Null
    }
    Copy-Item $helperBinary (Join-Path $aiCoolingDir "lens_helper.exe") -Force
    $defragDir = Join-Path $outRoot "defrag"
    if (-not (Test-Path $defragDir)) {
        New-Item -ItemType Directory -Force -Path $defragDir | Out-Null
    }
    Copy-Item $helperBinary (Join-Path $defragDir "lens_helper.exe") -Force
    $adblockDir = Join-Path $outRoot "adblock_master"
    if (-not (Test-Path $adblockDir)) {
        New-Item -ItemType Directory -Force -Path $adblockDir | Out-Null
    }
    Copy-Item $helperBinary (Join-Path $adblockDir "lens_helper.exe") -Force
    Write-Host "  -> lens_helper.exe (software_pm, ai_cooling, defrag, adblock_master)"
}

Write-Host "Tool packaging complete: $outRoot"
