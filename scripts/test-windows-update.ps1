param(
    [string]$Application = (Join-Path $PSScriptRoot '../target/windows/Tiny MD/tiny-md.exe')
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'setup-windows.ps1')
$workspaceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$testRoot = Join-Path $workspaceRoot ('target/update-smoke-' + [Guid]::NewGuid().ToString('N'))
$resolvedTestRoot = [IO.Path]::GetFullPath($testRoot)
if (!$resolvedTestRoot.StartsWith($workspaceRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Smoke workspace is outside the project'
}
$applicationPath = (Resolve-Path -LiteralPath $Application).Path
$applicationVersion = (& $applicationPath --version | Out-String).Trim() -replace '^Tiny MD ', ''
if ($applicationVersion -notmatch '^\d+\.\d+\.\d+([-+][0-9A-Za-z.+-]+)?$') { throw 'Version probe failed' }
$appFolder = Join-Path $resolvedTestRoot '中文 app''s $() & folder'
$payloadFolder = Join-Path $resolvedTestRoot 'payload'
New-Item -ItemType Directory -Path $appFolder, $payloadFolder | Out-Null
$fixtureSource = Join-Path $resolvedTestRoot 'fixture.rs'
# A benign fixture exercises replacement and restart without opening GPUI or
# touching the user's installation. Its only output is a marker beside itself.
@'
#![windows_subsystem = "windows"]
fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("Tiny MD {}", env!("TINY_MD_UPDATE_SMOKE_VERSION"));
    } else {
        let path = std::env::current_exe().unwrap();
        std::fs::write(path.parent().unwrap().join("restarted.txt"), b"restarted").unwrap();
    }
}
'@ | Set-Content -LiteralPath $fixtureSource -Encoding utf8
$currentExecutable = Join-Path $appFolder 'tiny-md.exe'
$payloadExecutable = Join-Path $payloadFolder 'tiny-md.exe'
$env:TINY_MD_UPDATE_SMOKE_VERSION = '0.0.0'
rustc --edition=2024 -C target-feature=+crt-static $fixtureSource -o $currentExecutable
if ($LASTEXITCODE -ne 0) { throw 'Old fixture compilation failed' }
$env:TINY_MD_UPDATE_SMOKE_VERSION = $applicationVersion
rustc --edition=2024 -C target-feature=+crt-static $fixtureSource -o $payloadExecutable
if ($LASTEXITCODE -ne 0) { throw 'New fixture compilation failed' }
Remove-Item Env:TINY_MD_UPDATE_SMOKE_VERSION
$temporaryPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
$helperRoot = [IO.Path]::GetFullPath((Join-Path $temporaryPrefix ('tiny-md-update-smoke-' + [Guid]::NewGuid().ToString('N'))))
if (!$helperRoot.StartsWith($temporaryPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe helper root' }
New-Item -ItemType Directory -Path $helperRoot | Out-Null
$archive = Join-Path $helperRoot "tiny-md-v$applicationVersion-windows-x64-portable.zip"
Compress-Archive -LiteralPath $payloadExecutable -DestinationPath $archive
$helper = Join-Path $helperRoot 'tiny-md-update-helper.exe'
Copy-Item -LiteralPath $applicationPath -Destination $helper
$parent = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoLogo', '-NoProfile', '-NonInteractive', '-Command', 'Start-Sleep -Seconds 2' -WindowStyle Hidden -PassThru
$plan = Join-Path $helperRoot 'install-plan.json'
@{
    process = $parent.Id
    current = $currentExecutable
    archive = $archive
    digest = 'sha256:' + (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    size = (Get-Item -LiteralPath $archive).Length
    version = 'v' + $applicationVersion
    kind = 'portable'
} | ConvertTo-Json | Set-Content -LiteralPath $plan -Encoding utf8NoBOM
$processInfo = [Diagnostics.ProcessStartInfo]::new($helper)
$processInfo.UseShellExecute = $false
$processInfo.CreateNoWindow = $true
$processInfo.ArgumentList.Add('--apply-update')
$processInfo.ArgumentList.Add($plan)
$processInfo.RedirectStandardOutput = $true
$processInfo.RedirectStandardError = $true
$update = [Diagnostics.Process]::Start($processInfo)
if (!$update.WaitForExit(30000)) { throw "Updater timed out; evidence: $helperRoot" }
$updateOutput = $update.StandardOutput.ReadToEnd() + $update.StandardError.ReadToEnd()
if ($update.ExitCode -ne 0) { throw "Helper failed: $updateOutput; evidence: $helperRoot" }
$deadline = [DateTime]::UtcNow.AddSeconds(10)
while (!(Test-Path -LiteralPath (Join-Path $appFolder 'restarted.txt')) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if (!(Test-Path -LiteralPath (Join-Path $appFolder 'restarted.txt'))) { throw 'Updated application was not restarted' }
if ((Get-FileHash -LiteralPath $currentExecutable).Hash -ne (Get-FileHash -LiteralPath $payloadExecutable).Hash) { throw 'The old executable was not replaced' }
while ((Test-Path -LiteralPath $helperRoot) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if (Test-Path -LiteralPath $helperRoot) { throw "Temporary update files were not cleaned: $helperRoot" }
Write-Output 'Windows portable helper: replacement, version check, restart and cleanup passed'
Write-Output "Isolated fixture evidence: $resolvedTestRoot"
