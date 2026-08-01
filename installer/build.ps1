# Build AAD, stage a portable package, zip it, and (if Inno Setup is installed)
# compile the Windows installer.
#
#   powershell -ExecutionPolicy Bypass -File installer\build.ps1
#
# Artifacts land in installer\Output\ :
#   * AAD-portable-<ver>.zip   — unzip anywhere, run "AAD.exe app"
#   * AAD-Setup-<ver>.exe      — only when Inno Setup 6 (iscc) is available
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
$root = Split-Path -Parent $here
$ver  = "0.0.0"

Write-Host "==> Building release (aadt-cli --features app)..."
Push-Location $root
try { cargo build --release -p aadt-cli --features app } finally { Pop-Location }

$exe = Join-Path $root "target\release\aadt.exe"
if (-not (Test-Path $exe)) { throw "build did not produce $exe" }

# Stage a portable tree.
$stageRoot = Join-Path $here "stage"
$stage = Join-Path $stageRoot "AAD"
if (Test-Path $stageRoot) { Remove-Item $stageRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $exe (Join-Path $stage "AAD.exe")
Copy-Item (Join-Path $root "libraries") (Join-Path $stage "libraries") -Recurse
Copy-Item (Join-Path $root "README.md") $stage
Copy-Item (Join-Path $root "NOTICE.md") $stage

$out = Join-Path $here "Output"
New-Item -ItemType Directory -Force -Path $out | Out-Null

# Portable zip (zero-dependency artifact).
$zip = Join-Path $out "AAD-portable-$ver.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Write-Host "==> Portable package: $zip"

# Full installer, if Inno Setup is available.
$iscc = $null
$cmd = Get-Command iscc -ErrorAction SilentlyContinue
if ($cmd) { $iscc = $cmd.Source }
else {
    foreach ($p in @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
                      "$env:ProgramFiles\Inno Setup 6\ISCC.exe")) {
        if ($p -and (Test-Path $p)) { $iscc = $p; break }
    }
}
if ($iscc) {
    Write-Host "==> Compiling installer with Inno Setup..."
    & $iscc (Join-Path $here "aad.iss")
    Write-Host "==> Installer: $out\AAD-Setup-$ver.exe"
} else {
    Write-Host "==> Inno Setup (iscc) not found - skipped setup.exe."
    Write-Host "    Install Inno Setup 6 (https://jrsoftware.org/isinfo.php) and re-run,"
    Write-Host "    or distribute the portable zip above."
}
