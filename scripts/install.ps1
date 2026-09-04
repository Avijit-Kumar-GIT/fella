# Fella installer for Windows. Downloads the latest GitHub Release build and
# runs it. An unofficial convenience the same thing you'd do by hand from
# https://github.com/Avijit-Kumar-GIT/fella/releases
#
#   irm https://lilfella.app/install.ps1 | iex
#
# Experimental: builds are unsigned, and this is not yet smoke-tested on every
# OS. Read the script before piping it to iex.

#Requires -Version 5
$ErrorActionPreference = 'Stop'

$repo = 'Avijit-Kumar-GIT/fella'
$manual = "https://github.com/$repo/releases/latest"
$headers = @{ 'User-Agent' = 'fella-install'; 'Accept' = 'application/vnd.github+json' }

function Fail($msg) {
    Write-Host "install: $msg" -ForegroundColor Red
    Write-Host "Get it by hand: $manual"
    exit 1
}

Write-Host 'Finding the latest Fella release...'
try {
    $rel = Invoke-RestMethod -UseBasicParsing -Headers $headers `
        "https://api.github.com/repos/$repo/releases/latest"
} catch {
    Fail 'no published release yet'
}

$asset = $rel.assets | Where-Object { $_.name -like '*-setup.exe' } | Select-Object -First 1
if (-not $asset) {
    $asset = $rel.assets | Where-Object { $_.name -like '*.msi' } | Select-Object -First 1
}
if (-not $asset) { Fail 'no Windows installer in the latest release' }

$tmp = Join-Path $env:TEMP $asset.name
Write-Host "Downloading $($asset.name)..."
Invoke-WebRequest -UseBasicParsing $asset.browser_download_url -OutFile $tmp

# Builds are unsigned verify against the release's SHA256SUMS. A missing
# SHA256SUMS is a loud warning; a mismatch is fatal.
$sums = $rel.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
if ($sums) {
    $sumsText = (Invoke-WebRequest -UseBasicParsing $sums.browser_download_url).Content
    $line = ($sumsText -split "`n") | Where-Object { $_ -match "[ \*]$([regex]::Escape($asset.name))\s*$" } | Select-Object -First 1
    if (-not $line) { Remove-Item $tmp -ErrorAction SilentlyContinue; Fail "SHA256SUMS has no entry for $($asset.name)" }
    $want = ($line -split '\s+')[0].ToLower()
    $got = (Get-FileHash -Algorithm SHA256 $tmp).Hash.ToLower()
    if ($want -ne $got) { Remove-Item $tmp -ErrorAction SilentlyContinue; Fail "checksum mismatch for $($asset.name) (expected $want, got $got) nothing installed" }
    Write-Host "Checksum OK: $($asset.name)"
} else {
    Write-Host 'install: no SHA256SUMS in this release skipping checksum check' -ForegroundColor Yellow
}

Write-Host 'Running the installer...'
if ($asset.name -like '*.msi') {
    Start-Process msiexec.exe -Wait -ArgumentList "/i `"$tmp`" /passive"
} else {
    Start-Process $tmp -Wait -ArgumentList '/S'   # NSIS silent install
}
Remove-Item $tmp -ErrorAction SilentlyContinue

Write-Host ''
Write-Host 'Fella installed. Find it in the Start menu.'
Write-Host 'Next: install Ollama (https://ollama.com) and run "ollama pull llama3.1"'
Write-Host 'for a local model, or open Fella and type /login to use a hosted one.'
