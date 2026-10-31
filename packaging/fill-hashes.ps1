# Fills in the zip's SHA256 checksum in the winget and Scoop files, from a PUBLISHED release's
# SHA256SUMS.txt (built by GitHub's release workflow). Run it after the release is published:
#   .\packaging\fill-hashes.ps1 0.1.0
param([Parameter(Mandatory)] [string] $Version)
$ErrorActionPreference = 'Stop'

$zip = "BitDinghy-$Version-windows-x64.zip"
$sums = (Invoke-WebRequest -UseBasicParsing `
    "https://github.com/Jack113/BitDinghy/releases/download/v$Version/SHA256SUMS.txt").Content
if ($sums -is [byte[]]) { $sums = [Text.Encoding]::ASCII.GetString($sums) }
$line = $sums -split "`n" | Where-Object { $_ -match "^([0-9a-f]{64})\s+\*?$([regex]::Escape($zip))\s*$" } | Select-Object -First 1
if (-not $line) { throw "No checksum for $zip in SHA256SUMS.txt" }
$hash = $line.Substring(0, 64)

$here = $PSScriptRoot
$scoop = Join-Path $here 'scoop\bitdinghy.json'
$winget = Join-Path $here 'winget\BitDinghy.BitDinghy.installer.yaml'
$utf8 = New-Object Text.UTF8Encoding $false
foreach ($file in $scoop, $winget) {
    $text = [IO.File]::ReadAllText($file)
    $text = $text -replace 'FILL-IN-WITH-fill-hashes\.ps1|\b[0-9a-fA-F]{64}\b', $hash
    [IO.File]::WriteAllText($file, $text, $utf8)
}
"SHA256 of ${zip}: $hash"
"Filled in: $scoop"
"Filled in: $winget"
