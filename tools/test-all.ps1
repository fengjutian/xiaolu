param(
    [string]$CargoTarget = ""
)

# Runs the test suite of every tool crate (plus the shared protocol crate).
# Uses the same shared target directory as build.ps1, so the dependencies are
# compiled once for the whole toolbox instead of once per crate.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$toolsRoot = Join-Path $root "tools"

if (-not $CargoTarget) {
    $CargoTarget = Join-Path $root ".cargo-target\tools"
}
$env:CARGO_TARGET_DIR = $CargoTarget
New-Item -ItemType Directory -Force -Path $CargoTarget | Out-Null

$failed = @()
$passed = 0
$total = 0

Get-ChildItem -Path $toolsRoot -Directory | Where-Object { $_.Name -notlike "_*" } | ForEach-Object {
    $project = Join-Path $_.FullName "Cargo.toml"
    if (-not (Test-Path $project)) { return }
    $total++
    Write-Host "Testing $($_.Name)..."
    cargo test --manifest-path $project --quiet
    if ($LASTEXITCODE -ne 0) {
        $failed += $_.Name
    } else {
        $passed++
    }
}

Write-Host ""
if ($failed.Count -gt 0) {
    Write-Host "FAILED: $($failed -join ', ') ($passed/$total passed)"
    exit 1
}
Write-Host "All $total crates passed."
