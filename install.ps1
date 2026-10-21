<#
.SYNOPSIS
  Installs repomix so that you can just type "repomix" in a terminal.

.DESCRIPTION
  Builds the release binary (if needed), copies it to an installation
  directory and adds that directory to the user PATH.

  The binary is statically linked, so it does not need the Visual C++
  redistributable, a Node runtime or anything else.

.PARAMETER InstallDir
  Where the binary is installed. Defaults to
  %LOCALAPPDATA%\Programs\repomix.

.PARAMETER Force
  Rebuild the binary even when it already exists.

.PARAMETER UseCargo
  Use "cargo install --path ." instead. That installs into
  %USERPROFILE%\.cargo\bin, which is usually on the PATH already.

.EXAMPLE
  .\install.ps1
  repomix --version
#>
param(
  [string]$InstallDir = "$env:LOCALAPPDATA\Programs\repomix",
  [switch]$Force,
  [switch]$UseCargo
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$crate = Join-Path $root "repomix-rs"
$exe = Join-Path $crate "target\release\repomix.exe"

function Info([string]$m) { Write-Host "==> $m" }
function Ok([string]$m)   { Write-Host "    $m" -ForegroundColor Green }
function Warn([string]$m) { Write-Host "    $m" -ForegroundColor Yellow }

if ($UseCargo) {
  Info "Installing with cargo"
  Push-Location $crate
  try { cargo install --path . --force 2>&1 | Out-Null } finally { Pop-Location }
  $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
  if (Test-Path (Join-Path $cargoBin "repomix.exe")) {
    Ok "installed into $cargoBin"
    if (($env:PATH -split ';') -notcontains $cargoBin) {
      Warn "$cargoBin is not on the PATH of this shell - close and reopen it"
    }
    exit 0
  }
  Warn "cargo install did not produce a binary, falling back to the copy install"
}

# --- build ----------------------------------------------------------------
if ($Force -or -not (Test-Path $exe)) {
  Info "Building the release binary (this takes a minute the first time)"
  # Cargo reads .cargo\config.toml from the *working directory*, so the
  # build has to run inside the crate - otherwise the binary is not linked
  # against the static C runtime.
  $p = Start-Process -FilePath "cargo" -ArgumentList @("build", "--release") `
        -WorkingDirectory $crate -NoNewWindow -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "the build failed (cargo exited with $($p.ExitCode))" }
  Ok "built"
} else {
  Info "Using the existing build (use -Force to rebuild)"
}

# --- install --------------------------------------------------------------
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$target = Join-Path $InstallDir "repomix.exe"
Copy-Item -LiteralPath $exe -Destination $target -Force
Ok "installed $target"

# --- PATH -----------------------------------------------------------------
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$parts = @($userPath -split ';' | Where-Object { $_ })
if ($parts -notcontains $InstallDir) {
  $newPath = (@($parts) + $InstallDir) -join ';'
  [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
  Ok "added $InstallDir to the user PATH"
} else {
  Ok "$InstallDir is already on the user PATH"
}

if (($env:PATH -split ';') -notcontains $InstallDir) {
  Write-Host ""
  Write-Host "The PATH of *this* shell is not updated yet." -ForegroundColor Yellow
  Write-Host "Open a new terminal (or run this to use it right away):" -ForegroundColor Yellow
  Write-Host ""
  Write-Host "    `$env:Path = `"$InstallDir;`$env:Path`""
  Write-Host ""
}

# --- verify ---------------------------------------------------------------
$version = & $target --version 2>&1
Ok "verified: $version"
Write-Host ""
Write-Host "Done. Open a new terminal and run:" -ForegroundColor Green
Write-Host ""
Write-Host "    repomix ."
Write-Host ""
