param(
  [string]$Target = "C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules",
  [int]$Runs = 3
)
# Measures the peak working set of the Rust binary and of the reference
# implementation.
$ErrorActionPreference = "Continue"
$Rs = if ($env:RS_BIN) { $env:RS_BIN } else { "C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe" }
$work = (Resolve-Path -LiteralPath $Target).Path

function Measure-Peak {
  param([string]$Exe, [string]$ArgLine)
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe
  $psi.Arguments = $ArgLine
  $psi.RedirectStandardOutput = $true
  $psi.UseShellExecute = $false
  $psi.WorkingDirectory = $work
  $proc = [System.Diagnostics.Process]::Start($psi)
  $peak = 0
  $reader = $proc.StandardOutput.ReadToEndAsync()
  while (-not $proc.HasExited) {
    try { $proc.Refresh(); if ($proc.WorkingSet64 -gt $peak) { $peak = $proc.WorkingSet64 } } catch { }
    Start-Sleep -Milliseconds 15
  }
  $proc.WaitForExit()
  $reader.Wait(2000) | Out-Null
  return $peak
}

$peaks = @()
for ($i = 0; $i -lt $Runs; $i++) {
  $peaks += (Measure-Peak -Exe $Rs -ArgLine "$Target --stdout")
}
$peaks = $peaks | Sort-Object
Write-Output ("rust peak working set : {0:N1} MB   (median of {1})" -f ($peaks[[int][Math]::Floor($Runs/2)]/1MB), $Runs)
