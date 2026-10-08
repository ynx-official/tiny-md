param([Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][int]$ParentProcess)
$ErrorActionPreference = 'Stop'
function Get-NormalizedPath([string]$Value) {
    if ($Value.StartsWith('\\?\UNC\')) { $Value = '\\' + $Value.Substring(8) }
    elseif ($Value.StartsWith('\\?\')) { $Value = $Value.Substring(4) }
    [IO.Path]::GetFullPath($Value)
}
$cleanupRoot = Get-NormalizedPath $Root
$temporaryRoot = (Get-NormalizedPath ([IO.Path]::GetTempPath())).TrimEnd('\') + '\'
if (!$cleanupRoot.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -or
    ![IO.Path]::GetFileName($cleanupRoot).StartsWith('tiny-md-update-')) {
    throw 'Cleanup target is outside the dedicated Tiny MD temporary directory'
}
if ((Get-Item -LiteralPath $cleanupRoot).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw 'Cleanup target cannot be a junction or symbolic link'
}
Wait-Process -Id $ParentProcess -Timeout 120 -ErrorAction SilentlyContinue
if (Get-Process -Id $ParentProcess -ErrorAction SilentlyContinue) { exit 1 }
Remove-Item -LiteralPath $cleanupRoot -Recurse -Force
