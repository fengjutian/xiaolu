# Shared source of truth for tool source fingerprints.
#
# A tool's fingerprint covers its own sources AND the sources of its path
# dependencies. 24 of the 25 tools link `../shared` (tool-protocol), so without
# the dependency part, editing the protocol crate would leave every packaged
# tool looking "fresh" while its binary was actually stale.
#
# Both build.ps1 (records the fingerprint) and verify-dist.ps1 (compares it)
# must compute the fingerprint the same way, so the implementation lives here
# instead of being copied into each script.

function Get-ToolFingerprint([string]$repoRoot, [string]$toolDir) {
    $files = Get-ToolSourceFiles -RepoRoot $repoRoot -ToolDir $toolDir
    $builder = New-Object System.Text.StringBuilder
    foreach ($file in ($files | Sort-Object)) {
        $hash = (Get-FileHash -Path $file -Algorithm SHA256).Hash
        [void]$builder.Append("$file|$hash`n")
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($builder.ToString())
    return ([System.BitConverter]::ToString($sha.ComputeHash($bytes)) -replace "-", "")
}

# Returns repo-root-relative paths of every file that determines the tool's
# binary: the tool's own sources plus those of its path dependencies
# (transitively).
function Get-ToolSourceFiles([string]$repoRoot, [string]$toolDir) {
    $collected = New-Object System.Collections.ArrayList
    $seen = @{}
    $queue = New-Object System.Collections.Queue
    $queue.Enqueue((Resolve-Path $toolDir).Path)

    while ($queue.Count -gt 0) {
        $dir = $queue.Dequeue()
        if ($seen.ContainsKey($dir)) { continue }
        $seen[$dir] = $true

        Get-ChildItem -Path $dir -Recurse -File | Where-Object {
            $relative = $_.FullName.Substring($dir.Length).TrimStart('\')
            $relative -notlike "target\*" -and
            ($_.Extension -eq ".rs" -or $_.Name -in @("Cargo.toml", "Cargo.lock", "tool.json"))
        } | ForEach-Object {
            $key = $_.FullName.Substring($repoRoot.Length).TrimStart('\')
            [void]$collected.Add($key)
        }

        $manifest = Join-Path $dir "Cargo.toml"
        if (Test-Path $manifest) {
            $text = [IO.File]::ReadAllText($manifest, [Text.Encoding]::UTF8)
            foreach ($match in [regex]::Matches($text, 'path\s*=\s*"([^"]+)"')) {
                $dependency = Join-Path $dir $match.Groups[1].Value
                if (Test-Path $dependency) {
                    $queue.Enqueue((Resolve-Path $dependency).Path)
                }
            }
        }
    }

    return $collected
}
