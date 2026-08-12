# Build + sideload AAD WinUI MSIX (M17.4)
# Requires: VS 2022 MSBuild, cargo release aadt_ffi.dll.
# Sideload needs the signing cert in Current User Trusted Root (one-time UI prompt)
# or Developer Mode + Add-AppDevPackage.ps1 from the AppPackages *_Test folder.

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
$winui = Join-Path $PSScriptRoot ".."
$sln = Join-Path $winui "AadWinui.sln"
$vs = "C:\Program Files\Microsoft Visual Studio\2022\Community\MSBuild\Current\Bin\MSBuild.exe"
if (-not (Test-Path $vs)) {
  throw "MSBuild not found at $vs"
}

Write-Host "== cargo build -p aadt-ffi --release =="
$vcvars = "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
cmd /c "`"$vcvars`" >nul && cd /d `"$root`" && cargo build -p aadt-ffi --release"
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

# Self-signed cert matching Package.appxmanifest Publisher=CN=AAD
$certName = "AAD WinUI MSIX"
$existing = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq "CN=AAD" -and $_.FriendlyName -eq $certName } | Select-Object -First 1
if (-not $existing) {
  Write-Host "== creating self-signed CN=AAD certificate =="
  $existing = New-SelfSignedCertificate `
    -Type Custom `
    -Subject "CN=AAD" `
    -KeyUsage DigitalSignature `
    -FriendlyName $certName `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
}
$thumb = $existing.Thumbprint
Write-Host "Signing thumbprint: $thumb"

$pkgDir = Join-Path $winui "AppPackages"
Write-Host "== MSBuild Release|x64 MSIX =="
& $vs $sln `
  /p:Configuration=Release `
  /p:Platform=x64 `
  /p:GenerateAppxPackageOnBuild=true `
  /p:AppxPackageSigningEnabled=true `
  /p:PackageCertificateThumbprint=$thumb `
  /p:AppxPackageDir="$pkgDir\\" `
  /m /v:minimal
if ($LASTEXITCODE -ne 0) { throw "MSBuild failed" }

$msix = Get-ChildItem $pkgDir -Recurse -Filter "*.msix" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $msix) {
  $msix = Get-ChildItem $pkgDir -Recurse -Include "*.msix","*.appx" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
}
if (-not $msix) { throw "No .msix/.appx under $pkgDir" }

$cerPath = Join-Path $msix.DirectoryName "AAD-WinUI.cer"
Export-Certificate -Cert $existing -FilePath $cerPath -Force | Out-Null
Write-Host "MSIX: $($msix.FullName)"
Write-Host "CER:  $cerPath"

# MSIX requires the publisher cert in Trusted Root (TrustedPeople alone → 0x800B0109).
$inRoot = Get-ChildItem Cert:\CurrentUser\Root -ErrorAction SilentlyContinue | Where-Object { $_.Thumbprint -eq $thumb }
if (-not $inRoot) {
  Write-Host ""
  Write-Host "Certificate is not in Current User\Trusted Root yet."
  Write-Host "One-time trust (Windows shows a security prompt — click Yes):"
  Write-Host "  certutil -user -addstore Root `"$cerPath`""
  Write-Host "Then re-run this script, or:"
  Write-Host "  Add-AppxPackage -Path `"$($msix.FullName)`""
  Write-Host "Alt: enable Developer Mode, then run Add-AppDevPackage.ps1 in the *_Test folder."
  Write-Host "Packaging succeeded; install skipped until Root trust."
  exit 0
}

Write-Host "== installing $($msix.FullName) =="
Get-AppxPackage -Name "Aad.Winui" -ErrorAction SilentlyContinue | Remove-AppxPackage -ErrorAction SilentlyContinue
try {
  Add-AppxPackage -Path $msix.FullName
  Write-Host "Installed. Launch from Start menu: AAD"
} catch {
  Write-Warning "Add-AppxPackage failed: $_"
  Write-Host "Trust Root via: certutil -user -addstore Root `"$cerPath`"  then retry Add-AppxPackage."
  exit 0
}
Write-Host "Package: $($msix.FullName)"
