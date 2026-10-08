# Dot-source this helper so compiler environment changes reach Cargo.
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'This script requires Windows' }

if (!(Get-Command cl.exe -ErrorAction SilentlyContinue) -or
    !(Get-Command rc.exe -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (!(Test-Path -LiteralPath $vswhere)) {
        throw 'Install Visual Studio Build Tools with Desktop development with C++ and Windows SDK'
    }
    $vsPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (!$vsPath) { throw 'Visual Studio C++ x64 build tools were not found' }
    Import-Module (Join-Path $vsPath 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
    Enter-VsDevShell -VsInstallPath $vsPath -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
}

foreach ($tool in @('cargo.exe', 'cl.exe', 'link.exe', 'rc.exe')) {
    if (!(Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Required build tool not found: $tool" }
}
if (!$env:GPUI_FXC_PATH -or !(Test-Path -LiteralPath $env:GPUI_FXC_PATH)) {
    $fxcCommand = Get-Command fxc.exe -ErrorAction SilentlyContinue
    if ($fxcCommand) {
        $env:GPUI_FXC_PATH = $fxcCommand.Source
    } else {
        $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
        $fxc = Get-ChildItem "$sdk/*/x64/fxc.exe" -ErrorAction SilentlyContinue |
            Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
            Select-Object -First 1
        if (!$fxc) { throw 'Windows SDK fxc.exe was not found; install the Windows SDK' }
        $env:GPUI_FXC_PATH = $fxc.FullName
    }
}
