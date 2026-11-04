# Makes the animated logo's pictures for the window: each drawing in tools\logo-layers\*.svg
# (cut from the website's animated logo, same shapes and colors) becomes a 300 x 148 PNG with a
# transparent background in crates\bitdinghy-app\assets\logo\. The window moves them itself
# (see crates\bitdinghy-app\src\logo.rs); egui can't play an SVG's own animation.
#
# Uses Microsoft Edge without a window, with a throwaway profile (your own Edge isn't touched).
# Run:  powershell -File tools\make-logo-layers.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$edge = @(
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $edge) { throw 'Microsoft Edge was not found.' }

$out = Join-Path $root 'crates\bitdinghy-app\assets\logo'
New-Item -ItemType Directory -Force $out | Out-Null
$profileDir = Join-Path $env:TEMP 'bitdinghy-logo-edge'

Get-ChildItem (Join-Path $PSScriptRoot 'logo-layers\*.svg') | ForEach-Object {
    $png = Join-Path $out ($_.BaseName + '.png')
    $url = 'file:///' + ($_.FullName -replace '\\', '/')
    Remove-Item $png -ErrorAction SilentlyContinue
    # Start-Process: Edge writes harmless messages to its error output, which PowerShell would
    # otherwise treat as a failure.
    $arguments = @('--headless', '--disable-gpu', '--hide-scrollbars', '--no-first-run',
        "--user-data-dir=`"$profileDir`"", '--default-background-color=00000000',
        '--window-size=300,148', "--screenshot=`"$png`"", "`"$url`"")
    Start-Process -FilePath $edge -ArgumentList $arguments -Wait -WindowStyle Hidden `
        -RedirectStandardError (Join-Path $env:TEMP 'bitdinghy-logo-edge.log')
    # Edge returns before the file is always flushed; wait for it briefly.
    $tries = 0
    while (-not (Test-Path $png) -and $tries -lt 50) { Start-Sleep -Milliseconds 100; $tries++ }
    if (-not (Test-Path $png)) { throw "Edge made no picture for $($_.Name)." }
    Write-Host "made $png"
}
Remove-Item -Recurse -Force $profileDir -ErrorAction SilentlyContinue
