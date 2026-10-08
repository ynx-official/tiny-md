param(
    [string]$Executable = (Join-Path $PSScriptRoot '../target/windows/Tiny MD/tiny-md.exe'),
    [string]$InstalledDirectory = (Join-Path $env:LOCALAPPDATA 'Programs/Tiny MD')
)

$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'verify-windows-exe.ps1') -Path $Executable
$sourceExecutable = (Resolve-Path -LiteralPath $Executable).Path
$installRoot = (Resolve-Path -LiteralPath $InstalledDirectory).Path
$installedExecutable = Join-Path $installRoot 'tiny-md.exe'
if (!(Test-Path -LiteralPath $installedExecutable -PathType Leaf)) {
    throw 'No existing Tiny MD installation was found in the specified directory'
}
if ($sourceExecutable -eq $installedExecutable) { throw 'The source is already the installed executable' }

$updateId = [Guid]::NewGuid().ToString('N')
$stagedExecutable = Join-Path $installRoot "tiny-md-update-$updateId.exe"
$backupExecutable = Join-Path $installRoot "tiny-md-before-$updateId.exe.bak"
Copy-Item -LiteralPath $sourceExecutable -Destination $stagedExecutable
$expectedHash = (Get-FileHash -LiteralPath $sourceExecutable).Hash
if ((Get-FileHash -LiteralPath $stagedExecutable).Hash -ne $expectedHash) {
    throw 'The staged executable does not match the build'
}

try {
    # Rename the old image instead of overwriting it. Running windows may keep
    # using that image; future shortcut launches use the new GUI executable.
    Move-Item -LiteralPath $installedExecutable -Destination $backupExecutable
    try {
        Move-Item -LiteralPath $stagedExecutable -Destination $installedExecutable
    } catch {
        Move-Item -LiteralPath $backupExecutable -Destination $installedExecutable
        throw
    }
} catch {
    throw "Unable to update Tiny MD: $($_.Exception.Message). The existing application was preserved; close its windows and retry."
}
if ((Get-FileHash -LiteralPath $installedExecutable).Hash -ne $expectedHash) {
    throw 'Installed executable verification failed; the previous version is retained in the backup'
}
Write-Output "Updated: $installedExecutable"
Write-Output "Previous executable: $backupExecutable"
Write-Output 'Close existing Tiny MD windows and reopen your shortcut to use the updated build.'
