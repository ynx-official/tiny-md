param(
    [string]$Document = '',
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
# Resolve relative document paths before changing to the repository directory.
if ($Document) { $Document = (Resolve-Path -LiteralPath $Document).Path }
Push-Location (Join-Path $PSScriptRoot '..')
try {
    . (Join-Path $PSScriptRoot 'setup-windows.ps1')
    $cargoArguments = @('run', '--locked', '-p', 'tiny-md')
    if ($Release) { $cargoArguments += '--release' }
    if ($Document) { $cargoArguments += @('--', $Document) }
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Tiny MD exited with code $LASTEXITCODE" }
} finally {
    Pop-Location
}
