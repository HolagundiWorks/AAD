# Full build + automated test set (WinUI shipping path)
#
# Usage:
#   .\scripts\full-build-and-test.ps1
#   .\scripts\full-build-and-test.ps1 -Launch
#   .\scripts\full-build-and-test.ps1 -SkipWinUI

param(
    [switch]$Launch,
    [switch]$SkipWinUI,
    [switch]$ReleaseWinUI
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Step($msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}

Step "cargo test -p aadt-ffi --lib (M18-M23 ABI suite)"
cargo test -p aadt-ffi --lib
if ($LASTEXITCODE -ne 0) { throw "aadt-ffi tests failed" }

Step "cargo test --workspace --lib"
cargo test --workspace --lib
if ($LASTEXITCODE -ne 0) { throw "workspace lib tests failed" }

Step "cargo build -p aadt-ffi --release"
cargo build -p aadt-ffi --release
if ($LASTEXITCODE -ne 0) { throw "aadt-ffi release build failed" }

$dll = Join-Path $Root "target\release\aadt_ffi.dll"
if (-not (Test-Path $dll)) { throw "missing $dll" }

if (-not $SkipWinUI) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $msbuild = & $vswhere -latest -requires Microsoft.Component.MSBuild -find "MSBuild\**\Bin\MSBuild.exe" |
        Select-Object -First 1
    if (-not $msbuild) {
        $msbuild = "C:\Program Files\Microsoft Visual Studio\2022\Community\MSBuild\Current\Bin\MSBuild.exe"
    }
    $cfg = if ($ReleaseWinUI) { "Release" } else { "Debug" }
    $sln = Join-Path $Root "native\aad-winui\winui\AadWinui.sln"
    Step "MSBuild $sln ($cfg|x64)"
    & $msbuild $sln /p:Configuration=$cfg /p:Platform=x64 /m /v:minimal /nologo
    if ($LASTEXITCODE -ne 0) { throw "WinUI build failed" }

    $out = Join-Path $Root "native\aad-winui\winui\x64\$cfg\AadWinui"
    if (-not (Test-Path $out)) {
        # Unpackaged Debug layout
        $out = Join-Path $Root "native\aad-winui\winui\x64\Debug\AadWinui"
    }
    New-Item -ItemType Directory -Force -Path $out | Out-Null
    Copy-Item $dll $out -Force
    Write-Host "Copied aadt_ffi.dll -> $out"

    if ($Launch) {
        $exe = Join-Path $out "AadWinui.exe"
        Get-Process -Name "AadWinui" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Step "Launch $exe"
        Start-Process -FilePath $exe -WorkingDirectory $out
    }
}

Step "Done"
Write-Host "Manual checklist: docs\qa\winui-m23-test-set.md" -ForegroundColor Green
