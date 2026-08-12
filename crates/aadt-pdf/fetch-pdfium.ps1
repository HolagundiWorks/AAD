# Download Pdfium native library for L3.3 / ADR-0019 (Windows x64).
# Places pdfium.dll next to this script's ../../../../target/release (and cwd).
# Source: https://github.com/bblanchon/pdfium-binaries

$ErrorActionPreference = "Stop"
$ver = "chromium%2F7206"
$url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$ver/pdfium-win-x64.tgz"
$tmp = Join-Path $env:TEMP "pdfium-aad"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$tgz = Join-Path $tmp "pdfium.tgz"
Write-Host "== download $url =="
Invoke-WebRequest -Uri $url -OutFile $tgz
Write-Host "== extract =="
tar -xzf $tgz -C $tmp
$dll = Get-ChildItem -Path $tmp -Recurse -Filter "pdfium.dll" | Select-Object -First 1
if (-not $dll) { throw "pdfium.dll not found in archive" }
$dests = @(
  (Join-Path $PSScriptRoot "..\..\..\..\target\release"),
  (Join-Path $PSScriptRoot "..\..\..\..\target\debug"),
  (Get-Location).Path
)
foreach ($d in $dests) {
  if (-not (Test-Path $d)) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
  Copy-Item $dll.FullName (Join-Path $d "pdfium.dll") -Force
  Write-Host "Copied -> $(Join-Path $d 'pdfium.dll')"
}
Write-Host "Done. Restart aadt app (gui feature) to Open PDF in Review."
