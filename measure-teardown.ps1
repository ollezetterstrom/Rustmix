param(
  [string]$Target = "C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules",
  [int]$Runs = 3
)
# Compares the wall clock of the process with the time it spends inside main(),
# to see how much the process start and the teardown of the address space cost.
$Rs = if ($env:RS_BIN) { $env:RS_BIN } else { "C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe" }
$work = (Resolve-Path -LiteralPath $Target).Path

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
}

foreach ($case in @(@{ label = "the corpus"; dir = $work }, @{ label = "a small tree"; dir = "C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\src" })) {
  $dir = (Resolve-Path -LiteralPath $case.dir).Path
  $walls = @()
  $mains = @()
  for ($i = 0; $i -lt $Runs; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-Timed -Exe $Rs -ArgLine "$dir --stdout" -WorkDir $dir
    $sw.Stop()
    $walls += $sw.Elapsed.TotalMilliseconds
    $m = [regex]::Match($script:Err, "total \(in main\)\s+([0-9.]+)ms")
    if ($m.Success) { $mains += [double]$m.Groups[1].Value }
  }
  $wall = ($walls | Sort-Object)[1]
  $main = ($mains | Sort-Object)[1]
  Write-Output ("{0,-14} wall={1,8:N1} ms   in main={2,8:N1} ms   outside={3,8:N1} ms   out={4} chars" -f $case.label, $wall, $main, ($wall - $main), $script:OutLen)
}
