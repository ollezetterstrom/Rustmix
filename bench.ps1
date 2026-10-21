param(
  [Parameter(Mandatory = $true)][string]$Target,
  [int]$Runs = 5,
  [string[]]$RepomixArgs = @(),
  [switch]$SkipJs
)

$ErrorActionPreference = "Continue"
$Rs = if ($env:RS_BIN) { $env:RS_BIN } else { "C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe" }
$script:LastMedian = 0

function Quote-Args {
  # NOTE: the parameter must NOT be called $Args - that collides with
  # PowerShell's automatic $args variable and silently yields an empty string.
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
  # CRITICAL: PowerShell's Set-Location/Push-Location does NOT change the native
  # process working directory ([Environment]::CurrentDirectory stays put), so a
  # child process started without an explicit WorkingDirectory inherits the
  # shell's original CWD - NOT the pushed-to directory. Always set it here.
  if ($WorkDir) { $psi.WorkingDirectory = $WorkDir }
  $proc = [System.Diagnostics.Process]::Start($psi)
  $script:LastOutLen = $proc.StandardOutput.ReadToEnd().Length
  $script:LastErr = $proc.StandardError.ReadToEnd()
  $proc.WaitForExit()
  $script:LastExit = $proc.ExitCode
}

function Measure-Runs {
  param([string]$Label, [string]$Exe, [string]$ArgLine, [string]$WorkDir, [switch]$ClearTokenCache)
  $times = @()
  for ($i = 0; $i -lt $Runs; $i++) {
    if ($ClearTokenCache) {
      $cache = Join-Path $env:TEMP "repomix\cache\token-counts.json"
      if (Test-Path -LiteralPath $cache) { Remove-Item -LiteralPath $cache -Force }
    }
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-Timed -Exe $Exe -ArgLine $ArgLine -WorkDir $WorkDir
    $sw.Stop()
    $times += $sw.Elapsed.TotalMilliseconds
  }
  $sorted = $times | Sort-Object
  $min = $sorted[0]
  $max = $sorted[-1]
  $median = $sorted[[int][Math]::Floor($Runs / 2)]
  $mean = ($times | Measure-Object -Average).Average
  Write-Host ("{0,-30} min={1,8:N1}ms  median={2,8:N1}ms  mean={3,8:N1}ms  max={4,8:N1}ms  out={5} chars exit={6}" -f $Label, $min, $median, $mean, $max, $script:LastOutLen, $script:LastExit)
  $script:LastMedian = $median
  $script:LastOutBytes = $script:LastOutLen
}

Write-Output "target : $Target"
Write-Output "runs   : $Runs"
Write-Output "args   : $($RepomixArgs -join ' ')"
Write-Output ("-" * 78)

# The corpus is always the process working directory. The packed paths are
# root-relative, so this is equivalent to passing the directory explicitly.
$work = (Resolve-Path -LiteralPath $Target).Path

# Cold start: process spawn + argument parsing only (best of a few runs).
$cold = @()
for ($i = 0; $i -lt 5; $i++) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  Invoke-Timed -Exe $Rs -ArgLine "--version" -WorkDir $work
  $sw.Stop()
  $cold += $sw.Elapsed.TotalMilliseconds
}
$coldSorted = $cold | Sort-Object
Write-Output ("{0,-30} min={1,8:N1}ms  median={2,8:N1}ms" -f "cold start (--version)", $coldSorted[0], $coldSorted[2])

$rsArgs = Quote-Args (@("--stdout") + $RepomixArgs)
Measure-Runs -Label "rust --stdout" -Exe $Rs -ArgLine $rsArgs -WorkDir $work
$rsMed = $script:LastMedian
$rsOut = $script:LastOutBytes

if (-not $SkipJs) {
  # npx is a .cmd shim, so it has to be run through cmd.exe.
  $jsArgs = "/c npx " + (Quote-Args (@("repomix", "--stdout") + $RepomixArgs))
  Measure-Runs -Label "js --stdout (warm cache)" -Exe "$env:ComSpec" -ArgLine $jsArgs -WorkDir $work
  $jsMed = $script:LastMedian
  $jsOut = $script:LastOutBytes
  Measure-Runs -Label "js --stdout (COLD cache)" -Exe "$env:ComSpec" -ArgLine $jsArgs -WorkDir $work -ClearTokenCache
  $jsColdMed = $script:LastMedian
  $jsColdOut = $script:LastOutBytes
  Write-Output ("-" * 78)
  Write-Output ("speedup (js warm / rust)     : {0:N2}x" -f ($jsMed / $rsMed))
  Write-Output ("speedup (js cold / rust)     : {0:N2}x" -f ($jsColdMed / $rsMed))
  # Sanity: both implementations must have packed the SAME workload.
  if ($rsOut -ne $jsOut -or $rsOut -ne $jsColdOut) {
    Write-Host ("!! WORKLOAD MISMATCH - rust={0} js={1} jsCold={2} chars. The runs did NOT pack the same tree." -f $rsOut, $jsOut, $jsColdOut) -ForegroundColor Red
  } else {
    Write-Host ("   workload verified: {0:N0} chars packed by both implementations" -f $rsOut) -ForegroundColor Green
  }
}
