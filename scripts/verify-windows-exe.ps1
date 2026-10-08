param(
    [Parameter(Mandatory = $true)]
    [string]$Path
)

$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Path).Path
$exeBytes = [IO.File]::ReadAllBytes($executablePath)
if ($exeBytes.Length -lt 64 -or [BitConverter]::ToUInt16($exeBytes, 0) -ne 0x5A4D) {
    throw 'The Windows executable is not a PE file'
}
$peOffset = [BitConverter]::ToInt32($exeBytes, 0x3C)
if ($peOffset -lt 0 -or $peOffset -gt $exeBytes.Length - 94 -or
    [BitConverter]::ToUInt32($exeBytes, $peOffset) -ne 0x00004550 -or
    [BitConverter]::ToUInt16($exeBytes, $peOffset + 4) -ne 0x8664 -or
    [BitConverter]::ToUInt16($exeBytes, $peOffset + 24) -ne 0x20B) {
    throw 'The Windows executable is not x64 PE32+'
}
if ([BitConverter]::ToUInt16($exeBytes, $peOffset + 24 + 68) -ne 2) {
    throw 'The Windows executable must use the GUI subsystem; a console build opens a CMD window'
}
Write-Output "Verified x64 Windows GUI executable: $executablePath"
