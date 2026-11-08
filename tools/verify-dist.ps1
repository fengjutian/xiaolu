param(
    [string]$Output = "tools-dist"
)

# Verifies that the packaged toolbox is complete and consistent with the sources.
# The host discovers tools by scanning <root>/<tool id>/tool.json and refuses to
# run a manifest whose entry executable is missing, so a half-populated package
# would silently ship an app with broken tools. Fail loudly instead.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$toolsRoot = Join-Path $root "tools"
$outRoot = Join-Path $root $Output

if (-not (Test-Path $outRoot)) {
    throw "Tool package directory not found: $outRoot (run 'npm run toolbox:build' first)"
}

# Fingerprint of the files that determine a tool's binary. Must match the
# fingerprint recorded at package time, otherwise the packaged executable is
# older than the sources and must not be shipped. Shared with build.ps1.
. (Join-Path $PSScriptRoot "fingerprint.ps1")

$problems = @()
$toolCount = 0
$elevatedTools = @()

$fingerprintPath = Join-Path $outRoot ".fingerprints.json"
$recorded = @{}
if (Test-Path $fingerprintPath) {
    $recordedText = [IO.File]::ReadAllText($fingerprintPath, [Text.Encoding]::UTF8)
    if ($recordedText.Trim()) {
        $parsed = $recordedText | ConvertFrom-Json
        $parsed.PSObject.Properties | ForEach-Object { $recorded[$_.Name] = $_.Value }
    }
}

# Packaged executable hashes: the source fingerprint proves the exe was built
# from these sources, but not that the exe itself is intact.
$binariesPath = Join-Path $outRoot ".binaries.json"
$recordedBinaries = @{}
if (Test-Path $binariesPath) {
    $binariesText = [IO.File]::ReadAllText($binariesPath, [Text.Encoding]::UTF8)
    if ($binariesText.Trim()) {
        $parsedBinaries = $binariesText | ConvertFrom-Json
        $parsedBinaries.PSObject.Properties | ForEach-Object { $recordedBinaries[$_.Name] = $_.Value }
    }
}

Get-ChildItem -Path $toolsRoot -Directory | Where-Object { $_.Name -notlike "_*" } | ForEach-Object {
    $tool = $_
    $manifestPath = Join-Path $tool.FullName "tool.json"
    if (-not (Test-Path $manifestPath)) { return }
    $manifestText = [IO.File]::ReadAllText($manifestPath, [Text.Encoding]::UTF8)
    $manifest = $manifestText | ConvertFrom-Json
    $toolCount++

    $destination = Join-Path $outRoot $manifest.id
    if (-not (Test-Path $destination)) {
        $problems += "$($manifest.id): missing package directory $destination"
        return
    }

    $entry = Join-Path $destination $manifest.entry
    if (-not (Test-Path $entry)) {
        $problems += "$($manifest.id): missing entry executable $($manifest.entry)"
    }

    $packagedManifest = Join-Path $destination "tool.json"
    if (-not (Test-Path $packagedManifest)) {
        $problems += "$($manifest.id): missing tool.json in package"
    } else {
        $packagedText = [IO.File]::ReadAllText($packagedManifest, [Text.Encoding]::UTF8)
        if ($packagedText -ne $manifestText) {
            $problems += "$($manifest.id): packaged tool.json differs from source (rebuild required)"
        }
    }

    # The packaged executable must have been built from the current sources.
    $current = Get-ToolFingerprint -RepoRoot $root -ToolDir $tool.FullName
    if (-not $recorded.ContainsKey($manifest.id)) {
        $problems += "$($manifest.id): no source fingerprint recorded (rebuild required)"
    } elseif ($recorded[$manifest.id] -ne $current) {
        $problems += "$($manifest.id): sources changed after packaging (rebuild required)"
    }

    # ...and it must still be the exact binary that was packaged.
    if (Test-Path $entry) {
        if (-not $recordedBinaries.ContainsKey($manifest.id)) {
            $problems += "$($manifest.id): no executable hash recorded (rebuild required)"
        } else {
            $actualHash = (Get-FileHash -Path $entry -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actualHash -ne $recordedBinaries[$manifest.id]) {
                $problems += "$($manifest.id): packaged executable hash mismatch (rebuild required)"
            }
        }
    }

    # A tool that references the elevation helper must be able to reach it.
    # The runtime lookup is: <tool dir>/lens_helper.exe first, then
    # <tool dir>/../lens_helper/lens_helper.exe. Verify the same contract.
    $mainRs = Join-Path $tool.FullName "src/main.rs"
    if ((Test-Path $mainRs) -and (Select-String -Path $mainRs -Pattern 'lens_helper\.exe' -Quiet)) {
        $elevatedTools += $manifest.id
        $sameDir = Join-Path $destination "lens_helper.exe"
        $sibling = Join-Path $destination "..\lens_helper\lens_helper.exe"
        if (-not (Test-Path $sameDir) -and -not (Test-Path $sibling)) {
            $problems += "$($manifest.id): needs the elevation helper but neither $sameDir nor $sibling exists"
        }
    }
}

if ($elevatedTools.Count -gt 0) {
    $helper = Join-Path $outRoot "lens_helper\lens_helper.exe"
    $packagedPerTool = $true
    foreach ($id in $elevatedTools) {
        if (-not (Test-Path (Join-Path (Join-Path $outRoot $id) "lens_helper.exe"))) { $packagedPerTool = $false }
    }
    if (-not (Test-Path $helper) -and -not $packagedPerTool) {
        $problems += "elevation helper not packaged at $helper and not copied per tool (required by: $($elevatedTools -join ', '))"
    }
    # The elevation helper has no manifest, so check its packaged hash directly:
    # a stale helper silently misses new operations added by the tools.
    if (Test-Path $helper) {
        if (-not $recordedBinaries.ContainsKey("lens_helper")) {
            $problems += "lens_helper: no executable hash recorded (rebuild required)"
        } else {
            $helperHash = (Get-FileHash -Path $helper -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($helperHash -ne $recordedBinaries["lens_helper"]) {
                $problems += "lens_helper: packaged executable hash mismatch (rebuild required)"
            }
        }
    }
}

if ($problems.Count -gt 0) {
    throw ("Tool package verification failed:`n  " + ($problems -join "`n  "))
}

Write-Host "Tool package verified: $toolCount tools complete in $Output"
Write-Host "  elevation helper reachable for: $($elevatedTools -join ', ')"
