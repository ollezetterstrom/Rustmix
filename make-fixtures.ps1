# Creates the fixture repositories used by the differential test suites.
#   testdata-git/       a git repository (diffs, logs, change frequency)
#   testdata-config/    a repomix config file
#   testdata-multi/     two directories, for the multi root test
#   testdata-encoding/  non UTF-8, CRLF and BOM files
# Everything is deterministic: the commit has a fixed author, date and
# message, so `git log` produces the same output on every machine.
$ErrorActionPreference = "Stop"
$root = "C:\Users\ozett\Documents\Projects\Rustmix"
$utf8 = New-Object System.Text.UTF8Encoding($false)

# --- testdata-git ---------------------------------------------------------
$gitDir = Join-Path $root "testdata-git"
Remove-Item $gitDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $gitDir | Out-Null

[System.IO.File]::WriteAllText("$gitDir\committed.txt", "first commit`n", $utf8)
[System.IO.File]::WriteAllText("$gitDir\modified.txt", "original content`n", $utf8)
[System.IO.File]::WriteAllText("$gitDir\staged.txt", "original content`n", $utf8)

Push-Location $gitDir
try {
  git init -q -b master
  git config user.name "Repomix Fixture"
  git config user.email "fixture@example.com"
  git config commit.gpgsign false
  # Keep the line endings exactly as they are, so that the fixture is
  # reproducible and git does not warn about the conversions.
  git config core.autocrlf false
  git add -A 2>&1 | Out-Null
  $env:GIT_AUTHOR_DATE = "2024-01-02T03:04:05+00:00"
  $env:GIT_COMMITTER_DATE = "2024-01-02T03:04:05+00:00"
  git commit -q -m "Initial fixture commit" 2>&1 | Out-Null
  Remove-Item Env:\GIT_AUTHOR_DATE, Env:\GIT_COMMITTER_DATE
  # An unstaged modification and a staged one, plus an untracked file.
  [System.IO.File]::WriteAllText("$gitDir\modified.txt", "changed in the working tree`n", $utf8)
  [System.IO.File]::WriteAllText("$gitDir\staged.txt", "changed and staged`n", $utf8)
  git add staged.txt 2>&1 | Out-Null
  [System.IO.File]::WriteAllText("$gitDir\untracked.txt", "not tracked at all`n", $utf8)
} finally { Pop-Location }

# --- testdata-config ------------------------------------------------------
$cfgDir = Join-Path $root "testdata-config"
Remove-Item $cfgDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path "$cfgDir\src" | Out-Null
[System.IO.File]::WriteAllText("$cfgDir\src\main.rs", ('fn main() { println!("hi"); }' + "`n"), $utf8)
[System.IO.File]::WriteAllText("$cfgDir\repomix.config.json", @'
{
  "output": {
    "fileSummary": false,
    "directoryStructure": false
  },
  "ignore": {
    "customPatterns": ["**/*.tmp"]
  }
}
'@, $utf8)

# --- testdata-multi -------------------------------------------------------
$multiDir = Join-Path $root "testdata-multi"
Remove-Item $multiDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path "$multiDir\alpha","$multiDir\beta" | Out-Null
[System.IO.File]::WriteAllText("$multiDir\alpha\one.txt", "alpha one`n", $utf8)
[System.IO.File]::WriteAllText("$multiDir\alpha\two.txt", "alpha two`n", $utf8)
[System.IO.File]::WriteAllText("$multiDir\beta\three.txt", "beta three`n", $utf8)

# --- testdata-encoding -----------------------------------------------------
$encDir = Join-Path $root "testdata-encoding"
Remove-Item $encDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $encDir | Out-Null
# A Latin-1 encoded file (invalid UTF-8, but no NUL byte).
[System.IO.File]::WriteAllBytes("$encDir\latin1.txt", [byte[]]@(0x63,0x61,0x66,0xE9,0x0A))
# A file with Windows line endings.
[System.IO.File]::WriteAllBytes("$encDir\crlf.txt", [byte[]]@(0x61,0x0D,0x0A,0x62,0x0D,0x0A))
# A file with a UTF-8 byte order mark.
[System.IO.File]::WriteAllBytes("$encDir\bom.txt", [byte[]]@(0xEF,0xBB,0xBF,0x68,0x69,0x0A))
# A file with a UTF-16 LE byte order mark.
[System.IO.File]::WriteAllBytes("$encDir\utf16le.txt", [byte[]]@(0xFF,0xFE,0x68,0x00,0x69,0x00,0x0A,0x00))
# Plain ASCII, as a control.
[System.IO.File]::WriteAllText("$encDir\plain.txt", "plain ascii`n", $utf8)

Write-Output "fixtures created:"
Get-ChildItem $root -Directory | Where-Object { $_.Name -like "testdata*" } | ForEach-Object {
  $n = (Get-ChildItem $_.FullName -Recurse -File -Force | Measure-Object).Count
  "  {0,-20} {1} files" -f $_.Name, $n
}
