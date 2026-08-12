# Build fixtures/winui-smoke.dxf via `aadt run` (for WinUI Open tests).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

cargo build -p aadt-cli --release
$aadt = Join-Path $Root "target\release\aadt.exe"
if (-not (Test-Path $aadt)) { throw "aadt.exe not found" }

$fix = Join-Path $Root "fixtures"
New-Item -ItemType Directory -Force -Path $fix | Out-Null
$dxf = Join-Path $fix "winui-smoke.dxf"
$script = Join-Path $fix "winui-smoke.aadt"

# Forward slashes avoid escaping issues in SAVE paths on Windows.
$dxfFwd = $dxf.Replace('\', '/')
@"
RECTANGLE 0 0 100 60
CIRCLE 50 30 18
LINE -20 -10 120 -10
TEXT 5 65 3 0 SMOKE
SAVE $dxfFwd
"@ | Set-Content -Path $script -Encoding utf8

& $aadt run $script
if ($LASTEXITCODE -ne 0) { throw "aadt run failed" }
if (-not (Test-Path $dxf)) { throw "expected fixture missing: $dxf" }

Write-Host "Fixture: $dxf"
Write-Host "Script:  $script"
