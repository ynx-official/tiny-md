param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference = 'Stop'
function Normalize-ApplicationPath([string]$Path) {
    if ($Path.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) { $Path = '\\' + $Path.Substring(8) }
    elseif ($Path.StartsWith('\\?\')) { $Path = $Path.Substring(4) }
    return [IO.Path]::GetFullPath($Path)
}
$targetPath = Normalize-ApplicationPath $Executable
$processName = [IO.Path]::GetFileNameWithoutExtension($targetPath)
foreach ($applicationProcess in (Get-Process -Name $processName -ErrorAction SilentlyContinue)) {
    if ($applicationProcess.Path -and
        (Normalize-ApplicationPath $applicationProcess.Path).Equals($targetPath, [StringComparison]::OrdinalIgnoreCase)) {
        exit 1
    }
}
