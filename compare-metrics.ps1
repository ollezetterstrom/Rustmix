param(
  [Parameter(Mandatory = $true)][string]$Dir,
  [Parameter(Mandatory = $true)][string]$RustExe,
  [string[]]$ExtraArgs = @()
)
# Compares the *numbers* of the console report (token counts, char counts,
# file counts, security findings) between the reference implementation and
# repomix-rs.
#
# The plain differential suite (compare-all.ps1) only compares stdout in
# --stdout mode, so the report - and with it the whole metrics phase - was
# never verified. This script closes that gap.
$ErrorActionPreference = "Continue"
$w = "C:\Users\ozett\Documents\Projects\Rustmix\work"
New-Item -ItemType Directory -Force -Path $w | Out-Null

# PowerShell's Set-Location does NOT change the native process working
# directory, so every child process gets an explicit WorkingDirectory.
$work = (Resolve-Path -LiteralPath $Dir).Path

function Quote-Args {
  # NOTE: the parameter must NOT be called $Args - that collides with
  # PowerShell's automatic $args variable and silently yields an empty string.
  param([string[]]$Values)
  ($Values | ForEach-Object {
    if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
  }) -join ' '
}

function Invoke-Capture {
  param([string]$Exe, [string]$ArgLine, [string]$WorkDir)
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
  return @{ Out = $stdout; Err = $stderr; Code = $proc.ExitCode }
}

$ESC = [string][char]27
function Strip-Ansi([string]$s) { $s -replace ($ESC + "\[[0-9;]*[A-Za-z]"), "" }

# The reference implementation formats the numbers with `toLocaleString()`,
# so the group separator depends on the machine's locale (this machine uses
# en-SE with a U+252C separator, a normal en-US machine uses a comma). Only
# the digits are compared.
function Digits([string]$s) { ($s -replace "[^0-9]", "") }

function Parse-Report([string]$raw) {
  $text = Strip-Ansi $raw
  $r = @{}
  $m = [regex]::Match($text, 'Total Files:\s*(\S+)\s+files')
  $r.Files = if ($m.Success) { Digits $m.Groups[1].Value } else { "<none>" }
  $m = [regex]::Match($text, 'Total Tokens:\s*(\S+)\s+tokens')
  $r.Tokens = if ($m.Success) { Digits $m.Groups[1].Value } else { "<none>" }
  $m = [regex]::Match($text, 'Total Chars:\s*(\S+)\s+chars')
  $r.Chars = if ($m.Success) { Digits $m.Groups[1].Value } else { "<none>" }
  # Per file: "N.  path (X tokens, Y chars, Z%)"
  $r.Files_detail = @{}
  foreach ($m in [regex]::Matches($text, '(?m)^\s*\d+\.\s+(\S+)\s+\((\S+)\s+tokens,\s*(\S+)\s+chars')) {
    $r.Files_detail[$m.Groups[1].Value] = "$(Digits $m.Groups[2].Value) tokens / $(Digits $m.Groups[3].Value) chars"
  }
  # Security: the number of suspicious files (or the "none detected" marker).
  $m = [regex]::Match($text, 'No suspicious files detected')
  if ($m.Success) {
    $r.Security = "none"
  } else {
    $m2 = [regex]::Match($text, '(?m)^\s*Security:\s*(.+)$')
    $r.Security = if ($m2.Success) { $m2.Groups[1].Value.Trim() } else { "<unknown>" }
  }
  return $r
}

function Compare-Field([string]$name, $a, $b) {
  if ("$a" -eq "$b") { return $true }
  Write-Host ("   {0,-14} JS={1}  RS={2}" -f $name, $a, $b) -ForegroundColor Red
  return $false
}

$cases = @(
  @{ name = "default";      args = @() },
  @{ name = "markdown";     args = @("--style","markdown") },
  @{ name = "json";         args = @("--style","json") },
  @{ name = "plain";        args = @("--style","plain") },
  @{ name = "parsable";     args = @("--parsable-style") },
  @{ name = "line numbers"; args = @("--output-show-line-numbers") },
  @{ name = "compress";     args = @("--compress") },
  @{ name = "top files 20"; args = @("--top-files-len","20") }
)

$pass = 0; $fail = 0
foreach ($c in $cases) {
  $argsList = @("-o", "metrics-out.xml") + $c.args + $ExtraArgs

  $jsResult = Invoke-Capture -Exe "$env:ComSpec" -ArgLine ("/c npx " + (Quote-Args (@("repomix") + $argsList))) -WorkDir $work
  $rsResult = Invoke-Capture -Exe $RustExe -ArgLine (Quote-Args $argsList) -WorkDir $work

  $js = Parse-Report $jsResult.Out
  $rs = Parse-Report $rsResult.Out

  $ok = $true
  $ok = (Compare-Field "Total Files" $js.Files $rs.Files) -and $ok
  $ok = (Compare-Field "Total Tokens" $js.Tokens $rs.Tokens) -and $ok
  $ok = (Compare-Field "Total Chars" $js.Chars $rs.Chars) -and $ok
  $ok = (Compare-Field "Security" $js.Security $rs.Security) -and $ok

  foreach ($k in $js.Files_detail.Keys) {
    $a = $js.Files_detail[$k]
    $b = if ($rs.Files_detail.ContainsKey($k)) { $rs.Files_detail[$k] } else { "<missing>" }
    $ok = (Compare-Field "file $k" $a $b) -and $ok
  }
  foreach ($k in $rs.Files_detail.Keys) {
    if (-not $js.Files_detail.ContainsKey($k)) {
      Write-Host ("   file {0} only in RS: {1}" -f $k, $rs.Files_detail[$k]) -ForegroundColor Red
      $ok = $false
    }
  }

  if ($ok) { Write-Output ("PASS  " + $c.name); $pass++ }
  else { Write-Output ("FAIL  " + $c.name); $fail++ }
}
Write-Output ""
Write-Output ("PASS: " + $pass + "  FAIL: " + $fail)
if ($fail -gt 0) { exit 1 }
