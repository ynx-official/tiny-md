param(
    [Parameter(Mandatory = $true)]
    [string]$InstallDir,
    [switch]$Uninstalled
)

$ErrorActionPreference = 'Stop'
$classes = 'Registry::HKEY_CURRENT_USER\Software\Classes'
$capabilities = 'Registry::HKEY_CURRENT_USER\Software\Tiny MD\Capabilities'
$registeredApps = 'Registry::HKEY_CURRENT_USER\Software\RegisteredApplications'
$progId = 'TinyMD.Markdown'
$command = '"' + (Join-Path ([IO.Path]::GetFullPath($InstallDir)) 'tiny-md.exe') + '" "%1"'
$privateKeys = @(
    "$classes\$progId",
    "$classes\Applications\tiny-md.exe",
    "$classes\SystemFileAssociations\.md\shell\TinyMD.Open",
    "$classes\SystemFileAssociations\.markdown\shell\TinyMD.Open",
    $capabilities
)

function Read-RegistryString([string]$Key, [string]$Name) {
    if (!(Test-Path -LiteralPath $Key)) { return $null }
    return (Get-Item -LiteralPath $Key).GetValue($Name, $null)
}

function Assert-RegistryString([string]$Key, [string]$Name, [string]$Expected) {
    if ((Read-RegistryString $Key $Name) -cne $Expected) {
        throw "Unexpected shell registration: $Key / $Name"
    }
}

if ($Uninstalled) {
    foreach ($key in $privateKeys) {
        if (Test-Path -LiteralPath $key) { throw "Shell registration remains after uninstall: $key" }
    }
    foreach ($extension in @('.md', '.markdown')) {
        if ($null -ne (Read-RegistryString "$classes\$extension\OpenWithProgids" $progId)) {
            throw "Open With registration remains after uninstall: $extension"
        }
    }
    if ($null -ne (Read-RegistryString $registeredApps 'Tiny MD')) {
        throw 'Default Apps registration remains after uninstall'
    }
    Write-Output 'Verified Tiny MD shell registration cleanup'
    return
}

foreach ($key in $privateKeys) {
    if (!(Test-Path -LiteralPath $key)) { throw "Missing shell registration: $key" }
}
Assert-RegistryString "$classes\$progId\shell\open\command" '' $command
Assert-RegistryString "$classes\Applications\tiny-md.exe\shell\open\command" '' $command
Assert-RegistryString $registeredApps 'Tiny MD' 'Software\Tiny MD\Capabilities'
Assert-RegistryString $capabilities 'ApplicationName' 'Tiny MD'
foreach ($extension in @('.md', '.markdown')) {
    $verb = "$classes\SystemFileAssociations\$extension\shell\TinyMD.Open"
    Assert-RegistryString $verb '' '通过 Tiny MD 打开'
    Assert-RegistryString "$verb\command" '' $command
    Assert-RegistryString $verb 'MultiSelectModel' 'Single'
    Assert-RegistryString "$classes\$extension\OpenWithProgids" $progId ''
    Assert-RegistryString "$capabilities\FileAssociations" $extension $progId
}
Write-Output 'Verified Tiny MD context menu, Open With and Default Apps registration'
