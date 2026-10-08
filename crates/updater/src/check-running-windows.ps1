param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference = 'Stop'
$targetPath = $Executable
if ($targetPath.StartsWith('\\?\')) { $targetPath = $targetPath.Substring(4) }
$targetPath = [IO.Path]::GetFullPath($targetPath)
$processName = [IO.Path]::GetFileNameWithoutExtension($targetPath)
foreach ($applicationProcess in (Get-Process -Name $processName -ErrorAction SilentlyContinue)) {
    if ($applicationProcess.Path -and
        [IO.Path]::GetFullPath($applicationProcess.Path).Equals($targetPath, [StringComparison]::OrdinalIgnoreCase)) {
        exit 1
    }
}
