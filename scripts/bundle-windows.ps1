param(
    [string]$Version = ''
)

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

if (!$Version) {
    $package = cargo pkgid -p tiny-md
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $Version = ($package -split '[@#]')[-1]
}
if ($Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$') {
    throw "Invalid version: $Version (expected X.Y.Z or X.Y.Z-beta.1)"
}

# Build with the static MSVC runtime so the app can be used without a separate
# Visual C++ runtime installation. Explicit target keeps host proc macros dynamic.
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --locked --release -p tiny-md --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$executable = 'target/x86_64-pc-windows-msvc/release/tiny-md.exe'
$bytes = [System.IO.File]::ReadAllBytes((Resolve-Path $executable))
if ($bytes.Length -lt 64 -or [BitConverter]::ToUInt16($bytes, 0) -ne 0x5A4D) {
    throw 'The Windows executable is not a PE file'
}
$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
if ($peOffset -lt 0 -or $peOffset -gt $bytes.Length - 6 -or
    [BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x00004550 -or
    [BitConverter]::ToUInt16($bytes, $peOffset + 4) -ne 0x8664) {
    throw 'The Windows executable is not x64'
}

$bundle = 'target/windows/Tiny MD'
New-Item -ItemType Directory -Force -Path $bundle, 'target/release-assets' | Out-Null
Copy-Item -LiteralPath $executable -Destination "$bundle/tiny-md.exe" -Force
Copy-Item -LiteralPath 'fixtures/welcome.md' -Destination "$bundle/welcome.md" -Force
Set-Content -LiteralPath "$bundle/version.txt" -Value $Version -Encoding utf8

$compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if ($compiler) {
    $compilerPath = $compiler.Source
} else {
    $compilerPath = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
}
if (!(Test-Path -LiteralPath $compilerPath)) {
    throw 'Inno Setup 6 is required to build the Windows installer'
}
$coreVersion = ($Version -split '[-+]')[0]
$sourceRoot = (Get-Location).Path
$output = Join-Path $sourceRoot 'target/release-assets'
& $compilerPath "/DAppVersion=$Version" "/DCoreVersion=$coreVersion" "/DSourceRoot=$sourceRoot" "/O$output" 'scripts/windows-installer.iss'
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$installer = Join-Path $output "tiny-md-v$Version-windows-x64-setup.exe"
if (!(Test-Path -LiteralPath $installer)) { throw 'The Windows installer was not generated' }
Write-Output "Built: $installer"
