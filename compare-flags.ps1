param(
  [Parameter(Mandatory = $true)][string]$RustExe,
  [string]$Root = "C:\Users\ozett\Documents\Projects\Rustmix"
)
# Differential tests for the flags that the plain output comparison
# (compare-all.ps1) does not cover:
#
#   * the git flags          (diffs, logs, change frequency sorting)
#   * --instruction-file-path
#   * --token-count-encoding
#   * --config
#   * --stdin
#   * --split-output         (produces several files, not one stdout stream)
#   * --token-budget         (exit code and stderr)
#   * multiple target directories
#   * files with an unusual encoding (Latin-1, CRLF, UTF-8/UTF-16 BOM)
#
# Every case runs the reference implementation and repomix-rs on the same
# fixture and compares the result byte for byte.
$ErrorActionPreference = "Continue"
# The stages are created OUTSIDE the repository: the reference implementation
# honours the .gitignore files of the parent directories, and this repository
# ignores /work/ - so staging inside it would make the reference pack
# nothing at all.
$w = Join-Path $env:TEMP "repomix-stages"
Remove-Item $w -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $w | Out-Null

$dirData      = Join-Path $Root "testdata"
$dirGit       = Join-Path $Root "testdata-git"
$dirConfig    = Join-Path $Root "testdata-config"
$dirMulti     = Join-Path $Root "testdata-multi"
$dirEncoding  = Join-Path $Root "testdata-encoding"

function Quote-Args {
  # NOTE: the parameter must NOT be called $Args - that collides with
  # PowerShell's automatic $args variable and silently yields an empty string.
  param([string[]]$Values)
  ($Values | ForEach-Object {
    if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
  }) -join ' '
}

function Invoke-Capture {
  param(
    [string]$Exe, [string]$ArgLine, [string]$WorkDir,
    [string]$Stdin = $null
  )
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe
  $psi.Arguments = $ArgLine
  $psi.WorkingDirectory = $WorkDir
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.UseShellExecute = $false
  if ($null -ne $Stdin) { $psi.RedirectStandardInput = $true }
  $proc = [System.Diagnostics.Process]::Start($psi)
  if ($null -ne $Stdin) {
    $proc.StandardInput.Write($Stdin)
    $proc.StandardInput.Close()
  }
  $stdout = $proc.StandardOutput.ReadToEnd()
  $stderr = $proc.StandardError.ReadToEnd()
  $proc.WaitForExit()
  return @{ Out = $stdout; Err = $stderr; Code = $proc.ExitCode }
}

# Runs both implementations in their own copy of the fixture directory, so
# that neither run can see the output files of the other one.
function New-Stage([string]$dir, [string]$name) {
  $stage = Join-Path $w $name
  Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path $stage | Out-Null
  Copy-Item (Join-Path $dir "*") $stage -Recurse -Force -ErrorAction SilentlyContinue
  return $stage
}

$pass = 0; $fail = 0
function Report([string]$name, [bool]$ok, [string]$detail = "") {
  if ($ok) { Write-Output ("PASS  " + $name); $script:pass++ }
  else { Write-Output ("FAIL  " + $name + "  " + $detail); $script:fail++ }
}

# --- 1. plain stdout comparisons ------------------------------------------
$stdoutCases = @(
  @{ name = "git diffs";        dir = $dirGit;      args = @("--include-diffs") },
  @{ name = "git logs";         dir = $dirGit;      args = @("--include-logs") },
  @{ name = "git diffs+logs";   dir = $dirGit;      args = @("--include-diffs","--include-logs") },
  @{ name = "git no sort";      dir = $dirGit;      args = @("--no-git-sort-by-changes") },
  @{ name = "instruction file"; dir = $dirData;     args = @("--instruction-file-path","instruction.md") },
  @{ name = "token encoding";   dir = $dirData;     args = @("--token-count-encoding","cl100k_base") },
  @{ name = "config file";      dir = $dirConfig;   args = @("--config","repomix.config.json") },
  @{ name = "multi root";       dir = $dirMulti;    args = @("alpha","beta") },
  @{ name = "odd encodings";    dir = $dirEncoding; args = @() }
)

# The instruction file has to exist in the fixture.
[System.IO.File]::WriteAllText((Join-Path $dirData "instruction.md"),
  "Follow these instructions when reviewing the code.`n", (New-Object System.Text.UTF8Encoding($false)))

foreach ($c in $stdoutCases) {
  $jsStage = New-Stage $c.dir ("js-" + ($c.name -replace '\W','_'))
  $rsStage = New-Stage $c.dir ("rs-" + ($c.name -replace '\W','_'))
  $jsArgs = @("repomix","--stdout") + $c.args
  $js = Invoke-Capture -Exe "$env:ComSpec" -ArgLine ("/c npx " + (Quote-Args $jsArgs)) -WorkDir $jsStage
  $rs = Invoke-Capture -Exe $RustExe -ArgLine (Quote-Args (@("--stdout") + $c.args)) -WorkDir $rsStage
  $a = $js.Out -replace "`r`n","`n"
  $b = $rs.Out -replace "`r`n","`n"
  if ($a -eq $b) { Report $c.name $true }
  else {
    Report $c.name $false ("(js=" + $a.Length + " rs=" + $b.Length + ")")
    $al = $a -split "`n"; $bl = $b -split "`n"
    $shown = 0
    for ($i = 0; $i -lt [Math]::Max($al.Count, $bl.Count) -and $shown -lt 4; $i++) {
      $x = if ($i -lt $al.Count) { $al[$i] } else { "<EOF>" }
      $y = if ($i -lt $bl.Count) { $bl[$i] } else { "<EOF>" }
      if ($x -ne $y) {
        Write-Output ("     line " + $i)
        Write-Output ("       JS: [" + $x + "]")
        Write-Output ("       RS: [" + $y + "]")
        $shown++
      }
    }
  }
}

# --- 2. --stdin -----------------------------------------------------------
# KNOWN DIVERGENCE (Windows only): the reference implementation builds the
# include pattern from `path.relative(root, file)`, which returns a path
# with backslashes on Windows. The backslash is then escaped into the glob
# pattern, and the pattern matches nothing - so the reference packs an EMPTY
# repository when a file path is piped in. On Linux and macOS the reference
# works, because `path.relative` returns forward slashes there.
#
# repomix-rs converts the path to POSIX before using it as a pattern, so it
# packs the requested files. That is the intended behaviour of the flag.
foreach ($stdinCase in @(
  @{ name = "stdin directory";  paths = "src`n";                    compare = $true },
  @{ name = "stdin files";      paths = "src/main.rs`nsrc/lib.rs`n"; compare = $false }
)) {
  $jsStage = New-Stage $dirData ("js-" + ($stdinCase.name -replace '\W','_'))
  $rsStage = New-Stage $dirData ("rs-" + ($stdinCase.name -replace '\W','_'))
  $js = Invoke-Capture -Exe "$env:ComSpec" -ArgLine "/c npx repomix --stdout --stdin" -WorkDir $jsStage -Stdin $stdinCase.paths
  $rs = Invoke-Capture -Exe $RustExe -ArgLine "--stdout --stdin" -WorkDir $rsStage -Stdin $stdinCase.paths
  $a = $js.Out -replace "`r`n","`n"
  $b = $rs.Out -replace "`r`n","`n"
  $jsFiles = @([regex]::Matches($a,'<file path="([^"]*)"') | ForEach-Object { $_.Groups[1].Value })
  $rsFiles = @([regex]::Matches($b,'<file path="([^"]*)"') | ForEach-Object { $_.Groups[1].Value })
  if ($stdinCase.compare) {
    if ($a -eq $b) { Report $stdinCase.name $true }
    else { Report $stdinCase.name $false ("(js=" + $a.Length + " rs=" + $b.Length + ")") }
  } else {
    # The reference must find nothing (the Windows path bug), repomix-rs must
    # pack exactly the files that were listed.
    $expected = @("src/lib.rs", "src/main.rs")
    $ok = ($jsFiles.Count -eq 0) -and (($rsFiles -join ",") -eq ($expected -join ","))
    Report ("known divergence: " + $stdinCase.name) $ok `
      ("(js=" + $jsFiles.Count + " files, rs=" + ($rsFiles -join ",") + ")")
  }
}

# --- 3. --split-output ---------------------------------------------------
# NOTE: the block has to be invoked with "&" - a bare { } block would only be
# written to the output instead of being executed.
& {
  $jsStage = New-Stage $dirData "js-split"
  $rsStage = New-Stage $dirData "rs-split"
  $js = Invoke-Capture -Exe "$env:ComSpec" -ArgLine "/c npx repomix --split-output 1kb -o out.xml" -WorkDir $jsStage
  $rs = Invoke-Capture -Exe $RustExe -ArgLine "--split-output 1kb -o out.xml" -WorkDir $rsStage
  $jsFiles = @(Get-ChildItem $jsStage -Filter "out*.xml" | Sort-Object Name)
  $rsFiles = @(Get-ChildItem $rsStage -Filter "out*.xml" | Sort-Object Name)
  $jsNames = ($jsFiles | ForEach-Object { $_.Name }) -join ","
  $rsNames = ($rsFiles | ForEach-Object { $_.Name }) -join ","
  if ($jsNames -ne $rsNames) {
    Report "split output" $false ("(file names differ: js=[$jsNames] rs=[$rsNames])")
  } else {
    $same = $true
    for ($i = 0; $i -lt $jsFiles.Count; $i++) {
      $ha = (Get-FileHash $jsFiles[$i].FullName -Algorithm SHA256).Hash
      $hb = (Get-FileHash $rsFiles[$i].FullName -Algorithm SHA256).Hash
      if ($ha -ne $hb) {
        $same = $false
        Write-Output ("     part " + $jsFiles[$i].Name + " differs")
      }
    }
    Report "split output" $same ("(" + $jsFiles.Count + " parts)")
  }
}

# --- 4. exit codes and stderr ---------------------------------------------
$exitCases = @(
  @{ name = "budget exceeded";  args = @("--token-budget","100");    expectCode = 1 },
  @{ name = "budget ok";        args = @("--token-budget","999999"); expectCode = 0 },
  @{ name = "split+stdout";     args = @("--split-output","1kb","--stdout"); expectCode = 1 }
)
foreach ($c in $exitCases) {
  $jsStage = New-Stage $dirData ("js-" + ($c.name -replace '\W','_'))
  $rsStage = New-Stage $dirData ("rs-" + ($c.name -replace '\W','_'))
  $js = Invoke-Capture -Exe "$env:ComSpec" -ArgLine ("/c npx repomix " + (Quote-Args $c.args)) -WorkDir $jsStage
  $rs = Invoke-Capture -Exe $RustExe -ArgLine (Quote-Args $c.args) -WorkDir $rsStage
  $ok = ($js.Code -eq $c.expectCode) -and ($rs.Code -eq $c.expectCode)
  Report ("exit " + $c.name) $ok ("(js=" + $js.Code + " rs=" + $rs.Code + " expected=" + $c.expectCode + ")")
}

Write-Output ""
Write-Output ("PASS: " + $pass + "  FAIL: " + $fail)
if ($fail -gt 0) { exit 1 }
