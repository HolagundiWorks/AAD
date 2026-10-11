<#
.SYNOPSIS
  Run the quality baseline commands and record what actually happened.

.DESCRIPTION
  Runs, in order: cargo check, cargo test, cargo fmt --check, cargo clippy -D warnings and (unless
  -SkipRelease) the release build. Each command's exit code, duration and the last lines of its
  output go to target/quality/baseline-<date>.md, with the machine and toolchain. Nothing here
  changes the repository: it only builds and tests. A failing command does not stop the run; the
  report says which failed, so a baseline is never reported as clean when it is not.

.EXAMPLE
  powershell -File tools/quality/baseline.ps1
  powershell -File tools/quality/baseline.ps1 -SkipRelease
#>
param([switch] $SkipRelease)
$ErrorActionPreference = 'Continue'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $Root
$env:PATH = "$HOME\.cargo\bin;$env:PATH"
$Out = Join-Path $Root 'target\quality'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Stamp = Get-Date -Format 'yyyy-MM-dd-HHmm'
$Report = Join-Path $Out "baseline-$Stamp.md"

$commands = @(
  @{ Name = 'check';   Cmd = 'cargo check --workspace --all-targets' },
  @{ Name = 'test';    Cmd = 'cargo test --workspace --all-targets --no-fail-fast' },
  @{ Name = 'fmt';     Cmd = 'cargo fmt --all -- --check' },
  @{ Name = 'clippy';  Cmd = 'cargo clippy --workspace --all-targets -- -D warnings' }
)
if (-not $SkipRelease) { $commands += @{ Name = 'release'; Cmd = 'cargo build --workspace --release' } }

$lines = @()
$lines += "# Quality baseline $Stamp"
$lines += ''
$lines += "- Commit: $(git rev-parse --short HEAD) ($(git status --porcelain | Measure-Object | ForEach-Object Count) uncommitted paths)"
$lines += "- rustc: $(rustc -V); cargo: $(cargo -V)"
$lines += "- OS: $((Get-CimInstance Win32_OperatingSystem).Caption) $((Get-CimInstance Win32_OperatingSystem).Version); CPU threads: $env:NUMBER_OF_PROCESSORS; RAM: $([math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB, 1)) GB"
$lines += ''
$lines += '| Step | Command | Exit | Seconds |'
$lines += '|---|---|---|---|'
$details = @()
foreach ($c in $commands) {
  Write-Output "==> $($c.Cmd)"
  $log = Join-Path $Out "$($c.Name)-$Stamp.log"
  $sw = [Diagnostics.Stopwatch]::StartNew()
  cmd /c "$($c.Cmd) > `"$log`" 2>&1"
  $code = $LASTEXITCODE
  $sw.Stop()
  $lines += "| $($c.Name) | ``$($c.Cmd)`` | $code | $([math]::Round($sw.Elapsed.TotalSeconds, 1)) |"
  $tail = Get-Content $log -Tail 12 -ErrorAction SilentlyContinue
  $details += ''
  $details += "## $($c.Name) (exit $code)"
  $details += '```text'
  $details += $tail
  $details += '```'
  if ($c.Name -eq 'test') {
    $passed = 0; $failed = 0
    foreach ($l in Get-Content $log) {
      if ($l -match 'test result: .* (\d+) passed; (\d+) failed') { $passed += [int]$Matches[1]; $failed += [int]$Matches[2] }
    }
    $details += ''
    $details += "Harness totals: $passed passed, $failed failed."
  }
}
($lines + $details) | Set-Content -Path $Report -Encoding utf8
Write-Output "report: $Report"
