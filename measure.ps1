param(
  [string]$Target = "C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules",
  [int]$Runs = 5,
  [string[]]$ExtraArgs = @()
)

# Measures the Rust binary the same way bench.ps1 does (a pipe, not a file),
# but also reports the phase breakdown and the wall clock of the whole
# process.  Usage:
#   .\measure.ps1 -Runs 3
#   .\measure.ps1 -Runs 3 -ExtraArgs "--style","markdown"

$ErrorActionPreference = "Continue"
$Rs = if ($env:RS_BIN) { $env:RS_BIN } else { "C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe" }
$work = (Resolve-Path -LiteralPath $Target).Path

function Quote-Args {
  param([string[]]$Values)
  ($Values | ForEach-Object {
    if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
  }) -join ' '
}

function Invoke-Timed {
  param([string]$Exe, [string]$ArgLine, [string]$WorkDir)
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe
  $psi.Arguments = $ArgLine
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.UseShellExecute = $false
  $psi.WorkingDirectory = $WorkDir
  $psi.EnvironmentVariables["REPOMIX_TIMING"] = "1"
  $proc = [System.Diagnostics.Process]::Start($psi)
  $script:OutLen = $proc.StandardOutput.ReadToEnd().Length
  $script:Err = $proc.StandardError.ReadToEnd()
  $proc.WaitForExit()
  $script:ExitCode = $proc.ExitCode
}

function Measure-Set {
  param([string]$Label, [string[]]$ArgList, [int]$N = $Runs)
  $times = @()
  for ($i = 0; $i -lt $N; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-Timed -Exe $Rs -ArgLine (Quote-Args $ArgList) -WorkDir $work
    $sw.Stop()
    $times += $sw.Elapsed.TotalMilliseconds
    if ($i -eq $N - 1) { $script:LastErr = $script:Err }
  }
  $sorted = $times | Sort-Object
  $median = $sorted[[int][Math]::Floor($N / 2)]
  Write-Host ("{0,-34} min={1,8:N1}ms  median={2,8:N1}ms  max={3,8:N1}ms  out={4} chars" -f $Label, $sorted[0], $median, $sorted[-1], $script:OutLen)
  return $median
}

Write-Output "target : $work"
Write-Output ("-" * 78)

# 1. --stdout: the metrics phase is skipped entirely.
$m1 = Measure-Set -Label "stdout (no metrics)" -ArgList (@("--stdout") + $ExtraArgs)
Write-Output "  breakdown:"
$script:LastErr -split "`r?`n" | Where-Object { $_ -match "timing" } | ForEach-Object { "    $_" }

# 2. The default mode: the output goes to a file and the report is printed,
#    so the metrics phase runs.
Push-Location $work
$m2 = Measure-Set -Label "default (report + metrics)" -ArgList (@("-o", "$env:TEMP\repomix-measure.xml") + $ExtraArgs)
Pop-Location
Write-Output "  breakdown:"
$script:LastErr -split "`r?`n" | Where-Object { $_ -match "timing" } | ForEach-Object { "    $_" }

Write-Output ("-" * 78)
Write-Output ("stdout  : {0:N1} ms" -f $m1)
Write-Output ("default : {0:N1} ms   (the extra {1:N1} ms is the metrics phase + the file write)" -f $m2, ($m2 - $m1))
