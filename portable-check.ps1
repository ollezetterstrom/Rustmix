# Lists the DLLs that a PE executable imports from - the real answer to
# "can I copy this one file to another machine and run it?".
param([Parameter(Mandatory = $true)][string]$Path)

$bytes = [System.IO.File]::ReadAllBytes($Path)

function U16([int]$o) { [BitConverter]::ToUInt16($bytes, $o) }
function U32([int]$o) { [BitConverter]::ToUInt32($bytes, $o) }
function RvaToOffset([uint32]$rva, [int]$secCount) {
  for ($i = 0; $i -lt $secCount; $i++) {
    $s = $script:secTable + $i * 0x28
    $va = U32($s + 12); $vs = U32($s + 8); $raw = U32($s + 20); $rs = U32($s + 16)
    if ($rva -ge $va -and $rva -lt ($va + [Math]::Max($vs, $rs))) { return $raw + ($rva - $va) }
  }
  return -1
}

$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
$numSections = U16($peOffset + 6)
$optSize = U16($peOffset + 0x14)
$magic = U16($peOffset + 0x18)
$pe32plus = ($magic -eq 0x20B)
# The section table follows the optional header.
$script:secTable = $peOffset + 0x18 + $optSize

# The data directory of the imports is entry 1.
$ddOffset = $peOffset + 0x18 + $(if ($pe32plus) { 0x70 } else { 0x60 })
$importRva = U32($ddOffset + 8)
$importSize = U32($ddOffset + 12)

if ($importRva -eq 0) { Write-Output "no import table"; exit 0 }

$off = RvaToOffset $importRva $numSections
$names = @()
while ($true) {
  $nameRva = U32($off + 12)
  if ($nameRva -eq 0) { break }
  $nameOff = RvaToOffset $nameRva $numSections
  $end = $nameOff
  while ($bytes[$end] -ne 0) { $end++ }
  $names += [System.Text.Encoding]::ASCII.GetString($bytes, $nameOff, $end - $nameOff)
  $off += 20
}
Write-Output "imported DLLs ($($names.Count)):"
$names | Sort-Object | ForEach-Object { "  $_" }
$vc = $names | Where-Object { $_ -match 'vcruntime|ucrtbase|api-ms-win-crt|msvcr' }
if ($vc) {
  Write-Output ""
  Write-Output "=> the C runtime is DYNAMIC - the redistributable is required"
} else {
  Write-Output ""
  Write-Output "=> the C runtime is STATIC - the binary is self contained"
}
