param(
    [string]$Application = (Join-Path $PSScriptRoot '../target/windows/Tiny MD/tiny-md.exe'),
    [string]$Compiler = '',
    # Use the headless update_helper example for negative cases; the real app
    # intentionally shows a modal failure dialog after handing off its windows.
    [switch]$IncludeFailures,
    [string]$LegacyHelper = ''
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'setup-windows.ps1')
$workspaceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$testRoot = Join-Path $workspaceRoot ('target/installed-update-smoke-' + [Guid]::NewGuid().ToString('N'))
if (!$testRoot.StartsWith($workspaceRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test root' }
New-Item -ItemType Directory -Path $testRoot | Out-Null
$applicationPath = (Resolve-Path -LiteralPath $Application).Path
$applicationVersion = (& $applicationPath --version | Out-String).Trim() -replace '^Tiny MD ', ''
if ($applicationVersion -notmatch '^\d+\.\d+\.\d+([-+][0-9A-Za-z.+-]+)?$') { throw 'Version probe failed' }
if (!$Compiler) {
    $installedCompiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    $Compiler = if ($installedCompiler) { $installedCompiler.Source } else { Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe' }
}
$compilerPath = (Resolve-Path -LiteralPath $Compiler).Path
$source = Join-Path $testRoot 'fixture.rs'
@'
#![windows_subsystem = "windows"]
fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("Tiny MD {}", env!("TINY_MD_INSTALLED_SMOKE_VERSION"));
    } else if std::env::args().any(|arg| arg == "--hold") {
        std::thread::sleep(std::time::Duration::from_secs(90));
    } else {
        let exe = std::env::current_exe().unwrap();
        std::fs::write(exe.parent().unwrap().join("restarted.txt"), env!("TINY_MD_INSTALLED_SMOKE_VERSION")).unwrap();
    }
}
'@ | Set-Content -LiteralPath $source -Encoding utf8
$oldPayload = Join-Path $testRoot 'old.exe'
$newPayload = Join-Path $testRoot 'new.exe'
$previousVersion = [Environment]::GetEnvironmentVariable('TINY_MD_INSTALLED_SMOKE_VERSION')
try {
    foreach ($fixture in @(@('0.0.0', $oldPayload), @($applicationVersion, $newPayload))) {
        $env:TINY_MD_INSTALLED_SMOKE_VERSION = $fixture[0]
        rustc --edition=2024 -C target-feature=+crt-static $source -o $fixture[1]
        if ($LASTEXITCODE -ne 0) { throw 'Fixture compilation failed' }
    }
} finally {
    [Environment]::SetEnvironmentVariable('TINY_MD_INSTALLED_SMOKE_VERSION', $previousVersion)
}
Set-Content -LiteralPath (Join-Path $testRoot 'version.txt') -Value $applicationVersion
Set-Content -LiteralPath (Join-Path $testRoot 'welcome.md') -Value 'new welcome'
$compatibility = Join-Path $PSScriptRoot 'windows-update-compat.iss'
$installerScript = Join-Path $testRoot 'fixture.iss'
# No uninstaller, registry entries, shortcuts, or actual application are used.
@'
#ifndef ResultCode
  #define ResultCode 0
#endif
[Setup]
AppName=Tiny MD isolated installed update
AppVersion=0.0.1
DefaultDirName={tmp}\tiny-md-unused-fixture
Uninstallable=no
CreateUninstallRegKey=no
PrivilegesRequired=lowest
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
CloseApplications=no
Compression=none
[Files]
Source: "{#Payload}"; DestDir: "{app}"; DestName: "tiny-md.exe"; Flags: ignoreversion
Source: "version.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "welcome.md"; DestDir: "{app}"; Flags: ignoreversion
[Code]
#ifdef Compatibility
  #include Compatibility
#endif
function GetCustomSetupExitCode: Integer;
begin
  Result := {#ResultCode};
end;
'@ | Set-Content -LiteralPath $installerScript -Encoding utf8BOM
function Build-Fixture([string]$Name, [string]$Payload, [int]$ResultCode, [bool]$Compatible) {
    $arguments = @('/Q', "/O$testRoot", "/F$Name", "/DPayload=$Payload", "/DResultCode=$ResultCode")
    if ($Compatible) { $arguments += "/DCompatibility=$compatibility" }
    & $compilerPath @arguments $installerScript
    if ($LASTEXITCODE -ne 0) { throw "Installer fixture compilation failed: $Name" }
    return (Join-Path $testRoot "$Name.exe")
}
$plainSetup = Build-Fixture 'plain' $newPayload 0 $false
$compatibleSetup = Build-Fixture 'compatible' $newPayload 0 $true
# Exercise the old clients' exact installer arguments even on CI where the old
# application binary is unavailable. The compatibility include must fix /DIR.
$legacyDirectory = Join-Path $testRoot 'legacy 参数 中文 app''s $() & folder'
$legacyStart = [Diagnostics.ProcessStartInfo]::new($compatibleSetup)
$legacyStart.UseShellExecute = $false
$legacyStart.CreateNoWindow = $true
foreach ($argument in @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/SP-', '/NORESTART', '/CURRENTUSER', '/NOCLOSEAPPLICATIONS', ('/DIR=\\?\' + $legacyDirectory), ('/LOG=' + (Join-Path $testRoot 'legacy-installer.log')))) {
    $legacyStart.ArgumentList.Add($argument)
}
$legacyInstall = [Diagnostics.Process]::Start($legacyStart)
if (!$legacyInstall.WaitForExit(30000) -or $legacyInstall.ExitCode -ne 0) { throw 'Legacy /DIR compatibility failed' }
if ((Get-FileHash -LiteralPath (Join-Path $legacyDirectory 'tiny-md.exe')).Hash -ne (Get-FileHash -LiteralPath $newPayload).Hash) { throw 'Legacy installer wrote to the wrong directory' }
Write-Output 'legacy installer arguments passed: canonicalized /DIR accepted'
function Run-Case([string]$Name, [string]$Setup, [string]$HelperBinary, [string]$FailurePattern = '', [bool]$HoldOtherInstance = $false) {
    $appFolder = Join-Path $testRoot ("中文 app's `$() & folder-$Name")
    New-Item -ItemType Directory -Path $appFolder | Out-Null
    $current = Join-Path $appFolder 'tiny-md.exe'
    Copy-Item -LiteralPath $oldPayload -Destination $current
    Set-Content -LiteralPath (Join-Path $appFolder 'version.txt') -Value 'old metadata'
    Set-Content -LiteralPath (Join-Path $appFolder 'welcome.md') -Value 'old welcome'
    $temporaryPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $helperRoot = [IO.Path]::GetFullPath((Join-Path $temporaryPrefix ('tiny-md-update-installed-' + [Guid]::NewGuid().ToString('N'))))
    if (!$helperRoot.StartsWith($temporaryPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe helper root' }
    New-Item -ItemType Directory -Path $helperRoot | Out-Null
    $archive = Join-Path $helperRoot "tiny-md-v$applicationVersion-windows-x64-setup.exe"
    Copy-Item -LiteralPath $Setup -Destination $archive
    $helper = Join-Path $helperRoot 'tiny-md-update-helper.exe'
    Copy-Item -LiteralPath $HelperBinary -Destination $helper
    $parent = Start-Process -FilePath powershell.exe -ArgumentList '-NoLogo', '-NoProfile', '-NonInteractive', '-Command', 'Start-Sleep -Seconds 2' -WindowStyle Hidden -PassThru
    $other = $null
    if ($HoldOtherInstance) { $other = Start-Process -FilePath $current -ArgumentList '--hold' -WindowStyle Hidden -PassThru }
    $plan = Join-Path $helperRoot 'install-plan.json'
    @{
        process = $parent.Id
        current = '\\?\' + $current
        archive = $archive
        digest = 'sha256:' + (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        size = (Get-Item -LiteralPath $archive).Length
        version = 'v' + $applicationVersion
        kind = 'setup'
    } | ConvertTo-Json | Set-Content -LiteralPath $plan -Encoding utf8NoBOM
    $start = [Diagnostics.ProcessStartInfo]::new($helper)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.ArgumentList.Add('--apply-update')
    $start.ArgumentList.Add($plan)
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    try {
        $update = [Diagnostics.Process]::Start($start)
        if (!$update.WaitForExit(30000)) { throw "Helper timed out; evidence: $helperRoot" }
        $output = $update.StandardOutput.ReadToEnd() + $update.StandardError.ReadToEnd()
        if ($FailurePattern) {
            if ($update.ExitCode -eq 0 -or $output -notmatch $FailurePattern) { throw "Expected failure missing: $output" }
            if (!(Test-Path -LiteralPath $helperRoot)) { throw 'Failure evidence was removed' }
            if (!(Test-Path -LiteralPath (Join-Path $helperRoot 'helper-ready'))) { throw 'Expected failure after handshake' }
            if ((Get-FileHash -LiteralPath $current).Hash -ne (Get-FileHash -LiteralPath $oldPayload).Hash) { throw 'Old executable was not restored' }
            if ((Get-Content -LiteralPath (Join-Path $appFolder 'version.txt') -Raw).Trim() -ne 'old metadata') { throw 'Old metadata was not restored' }
            if ((Get-Content -LiteralPath (Join-Path $appFolder 'welcome.md') -Raw).Trim() -ne 'old welcome') { throw 'Old welcome was not restored' }
            if ($HoldOtherInstance) {
                if ($other.HasExited) { throw 'Another instance was closed' }
                if (Test-Path -LiteralPath (Join-Path $helperRoot 'installer.log')) { throw 'Installer ran with another instance open' }
            } else {
                if (!(Test-Path -LiteralPath (Join-Path $helperRoot 'installer.log'))) { throw 'Installer diagnostic log missing' }
                $restartDeadline = [DateTime]::UtcNow.AddSeconds(10)
                while (!(Test-Path -LiteralPath (Join-Path $appFolder 'restarted.txt')) -and [DateTime]::UtcNow -lt $restartDeadline) { Start-Sleep -Milliseconds 100 }
                if ((Get-Content -LiteralPath (Join-Path $appFolder 'restarted.txt') -Raw) -ne '0.0.0') { throw 'Old version was not restarted after recovery' }
            }
            Write-Output "$Name passed: failure detected, old files preserved; evidence: $helperRoot"
        } else {
            if ($update.ExitCode -ne 0) { throw "Helper failed: $output; evidence: $helperRoot" }
            $deadline = [DateTime]::UtcNow.AddSeconds(10)
            while (!(Test-Path -LiteralPath (Join-Path $appFolder 'restarted.txt')) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
            if ((Get-Content -LiteralPath (Join-Path $appFolder 'restarted.txt') -Raw) -ne $applicationVersion) { throw 'New version was not restarted' }
            if ((Get-FileHash -LiteralPath $current).Hash -ne (Get-FileHash -LiteralPath $newPayload).Hash) { throw 'Wrong executable installed' }
            while ((Test-Path -LiteralPath $helperRoot) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
            if (Test-Path -LiteralPath $helperRoot) { throw "Update staging not cleaned: $helperRoot" }
            Write-Output "$Name passed: installation, new-version restart and cleanup"
        }
    } finally {
        # Only stop the benign process started by this case; never enumerate/kill user apps.
        if ($other -and !$other.HasExited) { $other.Kill(); $other.WaitForExit() }
    }
}
Run-Case 'normalized-helper' $plainSetup $applicationPath
Run-Case 'compatible-installer' $compatibleSetup $applicationPath
if ($LegacyHelper) {
    Run-Case 'legacy-helper' $compatibleSetup (Resolve-Path -LiteralPath $LegacyHelper).Path
}
if ($IncludeFailures) {
    $failedSetup = Build-Fixture 'installer-failure' $newPayload 42 $false
    $wrongVersionSetup = Build-Fixture 'wrong-version' $oldPayload 0 $false
    Run-Case 'installer-failure' $failedSetup $applicationPath 'exit code: 42'
    Run-Case 'wrong-version' $wrongVersionSetup $applicationPath '实际版本与发布标签不一致'
    Run-Case 'other-instance' $plainSetup $applicationPath '仍有其他 Tiny MD 进程' $true
}
Write-Output "Isolated installer fixtures: $testRoot"
