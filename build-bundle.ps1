# Builds a single self-contained markdown bundle of the project:
#   CONTEXT.md + PROMPT.md + the full Rust source tree + the reference
#   TypeScript implementation files that define the output contract.
# NOTE: this script is deliberately pure ASCII so that Windows PowerShell 5.1
#       (which reads .ps1 files using the ANSI code page) can parse it.
param(
  [string]$Root = "C:\Users\ozett\Documents\Projects\Rustmix",
  [string]$Out  = "C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs-SOURCE.md"
)

$ErrorActionPreference = "Stop"
$enc = New-Object System.Text.UTF8Encoding($false)

$rsRoot = Join-Path $Root "repomix-rs"
$tsRoot = "C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules\repomix\lib"

$tsFiles = @(
  "core/output/outputStyles/xmlStyle.js",
  "core/output/outputStyles/markdownStyle.js",
  "core/output/outputStyles/plainStyle.js",
  "core/output/outputGenerate.js",
  "core/output/outputStyleDecorate.js",
  "core/output/outputStyleUtils.js",
  "core/output/outputSort.js",
  "core/file/fileSearch.js",
  "core/file/filePathSort.js",
  "core/file/fileTreeGenerate.js",
  "core/file/fileProcessContent.js",
  "core/file/fileProcess.js",
  "core/file/fileManipulate.js",
  "core/file/fileRead.js",
  "core/file/truncateBase64.js",
  "core/packager/rootDisplayPath.js",
  "core/metrics/calculateMetrics.js",
  "core/metrics/calculateOutputMetrics.js",
  "config/configSchema.js",
  "config/configLoad.js",
  "config/defaultIgnore.js",
  "core/file/fileCollect.js"
)

$sb = New-Object System.Text.StringBuilder
$bt = [string][char]96
$fence = $bt * 4

function Add-Line([string]$s = "") { [void]$sb.AppendLine($s) }

function Add-File([string]$path, [string]$lang) {
  Add-Line ($fence + $lang)
  Add-Line ""
  $text = [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes($path))
  $text = $text -replace "`r`n", "`n"
  Add-Line $text
  Add-Line ""
  Add-Line $fence
  Add-Line ""
}

Add-Line "# repomix-rs - Complete Engineering Bundle"
Add-Line ""
Add-Line "**Read this whole file before answering.** It contains, in order:"
Add-Line ""
Add-Line "| Part | Contents |"
Add-Line "|------|----------|"
Add-Line "| 1 | The task prompt - what you are being asked to produce |"
Add-Line "| 2 | Engineering context - architecture, measured performance, the byte-for-byte output-compatibility contract, risks, deliverable spec |"
Add-Line "| 3 | The complete Rust source tree (31 files) |"
Add-Line "| 4 | The reference TypeScript implementation (repomix@1.18.1) files that define the observable output |"
Add-Line ""
Add-Line "Everything needed to reason about the code and the compatibility contract is in this file."
Add-Line ""
Add-Line "---"
Add-Line ""

Add-Line "## Part 1 - The task"
Add-Line ""
$prompt = [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes((Join-Path $Root "PROMPT.md")))
Add-Line ($prompt -replace "`r`n", "`n")
Add-Line "---"
Add-Line ""

Add-Line "## Part 2 - Engineering context"
Add-Line ""
$ctx = [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes((Join-Path $Root "CONTEXT.md")))
Add-Line ($ctx -replace "`r`n", "`n")
Add-Line "---"
Add-Line ""

Add-Line "## Part 3 - Rust source tree (repomix-rs/src/)"
Add-Line ""
$rsFiles = Get-ChildItem -Recurse (Join-Path $rsRoot "src") -Filter "*.rs" | Sort-Object FullName
foreach ($f in $rsFiles) {
  $rel = $f.FullName.Substring($rsRoot.Length + 1).Replace("\", "/")
  Add-Line ("### " + $bt + $rel + $bt)
  Add-Line ""
  Add-File $f.FullName "rust"
}
Add-Line "---"
Add-Line ""

Add-Line "## Part 4 - Reference implementation (TypeScript, repomix@1.18.1)"
Add-Line ""
Add-Line "The original implementation. It is the **ground truth** for every observable"
Add-Line "behaviour. Read it when you need to confirm a detail of the output format or"
Add-Line "of the ignore/glob semantics."
Add-Line ""
foreach ($rel in $tsFiles) {
  $p = Join-Path $tsRoot ($rel -replace "/", "\")
  if (-not (Test-Path $p)) { continue }
  Add-Line ("### " + $bt + "repomix/lib/$rel" + $bt)
  Add-Line ""
  Add-File $p "js"
}

[System.IO.File]::WriteAllText($Out, $sb.ToString(), $enc)
$size = (Get-Item $Out).Length
$lines = ([System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes($Out))) -split "`n"
Write-Output ("wrote {0}" -f $Out)
Write-Output ("  bytes  : {0:N0}" -f $size)
Write-Output ("  lines  : {0:N0}" -f ($lines.Count - 1))
Write-Output ("  ~tokens: {0:N0}" -f ($size / 3.7))
