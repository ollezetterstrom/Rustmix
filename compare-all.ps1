param(
  [Parameter(Mandatory = $true)][string]$Dir,
  [Parameter(Mandatory = $true)][string]$RustExe
)
$ErrorActionPreference = "Continue"
$w = "C:\Users\ozett\Documents\Projects\Rustmix\work"
New-Item -ItemType Directory -Force -Path $w | Out-Null

# IMPORTANT: PowerShell's Set-Location does NOT change the native process
# working directory ([Environment]::CurrentDirectory stays put), so every child
# process must be started with an explicit WorkingDirectory - otherwise both
# implementations silently pack the wrong tree.
$work = (Resolve-Path -LiteralPath $Dir).Path

function Invoke-Capture {
  param([string]$Exe, [string]$ArgLine, [string]$WorkDir)
  $o = Join-Path $w "capture.out"
  $e = Join-Path $w "capture.err"
  if (Test-Path $o) { Remove-Item $o -Force }
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe
  $psi.Arguments = $ArgLine
  $psi.WorkingDirectory = $WorkDir
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.UseShellExecute = $false
  $proc = [System.Diagnostics.Process]::Start($psi)
  $stdout = $proc.StandardOutput.ReadToEnd()
  $stderr = $proc.StandardError.ReadToEnd()
  $proc.WaitForExit()
  [System.IO.File]::WriteAllText($o, $stdout, (New-Object System.Text.UTF8Encoding($false)))
  return @{ Out = $stdout; Err = $stderr; Code = $proc.ExitCode }
}
function Quote-Args {
  # NOTE: the parameter must NOT be called $Args - that collides with
  # PowerShell's automatic $args variable and silently yields an empty string.
  param([string[]]$Values)
  ($Values | ForEach-Object {
    if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
  }) -join ' '
}

$cases = @(
  @{ name = "xml default";        args = @() },
  @{ name = "markdown";           args = @("--style","markdown") },
  @{ name = "json";               args = @("--style","json") },
  @{ name = "plain";              args = @("--style","plain") },
  @{ name = "parsable xml";       args = @("--parsable-style") },
  @{ name = "no file summary";    args = @("--no-file-summary") },
  @{ name = "no dir structure";   args = @("--no-directory-structure") },
  @{ name = "no files";           args = @("--no-files") },
  @{ name = "line numbers";       args = @("--output-show-line-numbers") },
  @{ name = "remove comments";    args = @("--remove-comments") },
  @{ name = "remove empty lines"; args = @("--remove-empty-lines") },
  @{ name = "truncate base64";    args = @("--truncate-base64") },
  @{ name = "compress";           args = @("--compress") },
  @{ name = "include glob";       args = @("--include","src/**") },
  @{ name = "ignore glob";        args = @("--ignore","*.rs") },
  @{ name = "no default patterns";args = @("--no-default-patterns") },
  @{ name = "no gitignore";       args = @("--no-gitignore") },
  @{ name = "no dot ignore";      args = @("--no-dot-ignore") },
  @{ name = "header text";        args = @("--header-text","Hello world") },
  @{ name = "empty directories";  args = @("--include-empty-directories") },
  @{ name = "full dir structure"; args = @("--include","src/**","--include-full-directory-structure") },
  @{ name = "no security check";  args = @("--no-security-check") },
  @{ name = "cwd relative";       args = @("--output-file-path-style","cwd-relative") },
  @{ name = "markdown+compress";  args = @("--style","markdown","--compress") },
  @{ name = "json+parsable";      args = @("--style","json") },
  @{ name = "top files";          args = @("--top-files-len","20") }
)

$pass = 0; $fail = 0
foreach ($c in $cases) {
  $argsList = @("--stdout") + $c.args

  # TypeScript implementation (npx repomix) via cmd.exe.
  $jsArgLine = "/c npx " + (Quote-Args (@("repomix") + $argsList))
  $jsResult = Invoke-Capture -Exe "$env:ComSpec" -ArgLine $jsArgLine -WorkDir $work

  # Rust implementation.
  $rsResult = Invoke-Capture -Exe $RustExe -ArgLine (Quote-Args $argsList) -WorkDir $work

  $a = $jsResult.Out -replace "`r`n","`n"
  $b = $rsResult.Out -replace "`r`n","`n"
  if ($a -eq $b) {
    Write-Output ("PASS  " + $c.name)
    $pass++
  } else {
    Write-Output ("FAIL  " + $c.name + "  (js=" + $a.Length + " rs=" + $b.Length + ")")
    $fail++
    $al = $a -split "`n"; $bl = $b -split "`n"
    $max = [Math]::Max($al.Count, $bl.Count); $shown = 0
    for ($i = 0; $i -lt $max; $i++) {
      $x = if ($i -lt $al.Count) { $al[$i] } else { "<EOF>" }
      $y = if ($i -lt $bl.Count) { $bl[$i] } else { "<EOF>" }
      if ($x -ne $y) {
        Write-Output ("   line " + $i); Write-Output ("     JS: [" + $x + "]"); Write-Output ("     RS: [" + $y + "]")
        $shown++; if ($shown -ge 6) { break }
      }
    }
  }
}
Write-Output ""
Write-Output ("PASS: " + $pass + "  FAIL: " + $fail)
