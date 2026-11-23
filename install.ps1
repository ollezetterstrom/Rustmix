param(
  [string]$InstallDir = "$env:LOCALAPPDATA\Programs\repomix"
)

$ErrorActionPreference = "Stop"

function Info([string]$m) { Write-Host "==> $m" -ForegroundColor Cyan }
function Ok([string]$m)   { Write-Host "    $m" -ForegroundColor Green }
function Warn([string]$m) { Write-Host "    $m" -ForegroundColor Yellow }

Info "Installing repomix for Windows (x86_64)..."

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$targetExe = Join-Path $InstallDir "repomix.exe"
$zipUrl = "https://github.com/ollezetterstrom/Rustmix/releases/latest/download/repomix-x86_64-pc-windows-msvc.zip"
$tempZip = Join-Path $env:TEMP "repomix-latest.zip"
$tempExtract = Join-Path $env:TEMP "repomix-extract-$([System.Guid]::NewGuid().ToString('N'))"

try {
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
  Info "Downloading latest release from GitHub..."
  Invoke-WebRequest -Uri $zipUrl -OutFile $tempZip -UseBasicParsing

  Info "Extracting binary..."
  Expand-Archive -LiteralPath $tempZip -DestinationPath $tempExtract -Force
  $foundExe = Get-ChildItem -Path $tempExtract -Filter "repomix.exe" -Recurse | Select-Object -First 1
  if (-not $foundExe) {
    throw "repomix.exe not found in downloaded release archive."
  }

  Copy-Item -LiteralPath $foundExe.FullName -Destination $targetExe -Force
  Ok "Installed to $targetExe"
} finally {
  Remove-Item -LiteralPath $tempZip -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $tempExtract -Recurse -Force -ErrorAction SilentlyContinue
}

# --- Add to User PATH if not present ---
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$parts = @($userPath -split ';' | Where-Object { $_ })
if ($parts -notcontains $InstallDir) {
  $newPath = (@($parts) + $InstallDir) -join ';'
  [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
  Ok "Added $InstallDir to user PATH"
} else {
  Ok "$InstallDir is already on user PATH"
}

if (($env:PATH -split ';') -notcontains $InstallDir) {
  $env:PATH = "$InstallDir;$env:PATH"
}

# --- Verify ---
try {
  $ver = & $targetExe --version 2>&1
  Ok "Verified: $ver"
} catch {
  Warn "Could not verify executable"
}

Write-Host ""
Write-Host "Done! You can now run repomix from any terminal:" -ForegroundColor Green
Write-Host "    repomix ."
Write-Host ""
