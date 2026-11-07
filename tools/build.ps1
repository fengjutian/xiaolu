param(
    [string]$Configuration = "release",
    [string]$Output = "tools-dist"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$toolsRoot = Join-Path $root "tools"
$outRoot = Join-Path $root $Output

# Fingerprint of the files that determine a tool's binary. The packaged copy is
# only trusted while this still matches, so a source edit can never silently
# ship a stale executable.
function Get-ToolFingerprint([string]$toolDir) {
    $files = Get-ChildItem -Path $toolDir -Recurse -File | Where-Object {
        $relative = $_.FullName.Substring($toolDir.Length).TrimStart('\')
        $relative -notlike "target\*" -and
        ($_.Extension -eq ".rs" -or $_.Name -in @("Cargo.toml", "Cargo.lock", "tool.json"))
    } | Sort-Object { $_.FullName.Substring($toolDir.Length) }
    $builder = New-Object System.Text.StringBuilder
    foreach ($file in $files) {
        $relative = $file.FullName.Substring($toolDir.Length).TrimStart('\')
        $hash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash
        [void]$builder.Append("$relative|$hash`n")
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($builder.ToString())
    return ([System.BitConverter]::ToString($sha.ComputeHash($bytes)) -replace "-", "")
}

New-Item -ItemType Directory -Force -Path $outRoot | Out-Null

$fingerprints = @{}

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
    # Record the fingerprint so a later verify can tell a fresh package from a stale one.
    $fingerprints[$manifest.id] = Get-ToolFingerprint $tool.FullName
    # The elevation helper now lives once at <dist>/lens_helper; drop stale per-tool copies.
    $staleHelper = Join-Path $destination "lens_helper.exe"
    if (Test-Path $staleHelper) {
        Remove-Item -LiteralPath $staleHelper -Force
    }
    Write-Host "  -> $destination"
}

# lens_helper is not a toolbox-protocol tool, so it is built separately.
# It is placed once at <dist>/lens_helper/lens_helper.exe: every tool that needs
# elevation resolves it via <tool dir>/../lens_helper/lens_helper.exe, so one copy
# is enough and the package stays small.
$helperRoot = Join-Path $toolsRoot "lens_helper"
$helperProject = Join-Path $helperRoot "Cargo.toml"
if (Test-Path $helperProject) {
    Write-Host "Building lens_helper..."
    cargo build "--$Configuration" --manifest-path $helperProject
    if ($LASTEXITCODE -ne 0) { throw "Build failed for lens_helper" }
    $targetDir = if ($Configuration -eq "release") { "release" } else { "debug" }
    $helperBinary = Join-Path $helperRoot "target/$targetDir/lens_helper.exe"
    if (-not (Test-Path $helperBinary)) { throw "Missing binary: $helperBinary" }
    $helperDir = Join-Path $outRoot "lens_helper"
    New-Item -ItemType Directory -Force -Path $helperDir | Out-Null
    Copy-Item $helperBinary $helperDir -Force
    Write-Host "  -> $helperDir\lens_helper.exe"
}

Write-Host "Tool packaging complete: $outRoot"

# One package-level fingerprint map: tool id -> source fingerprint.
[IO.File]::WriteAllText(
    (Join-Path $outRoot ".fingerprints.json"),
    ($fingerprints | ConvertTo-Json -Compress),
    (New-Object System.Text.UTF8Encoding($false))
)

& (Join-Path $PSScriptRoot "verify-dist.ps1") -Output $Output
