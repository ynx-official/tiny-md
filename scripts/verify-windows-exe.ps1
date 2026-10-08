param(
    [Parameter(Mandatory = $true)]
    [string]$Path
)

$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Path).Path
$exeBytes = [IO.File]::ReadAllBytes($executablePath)
if ($exeBytes.Length -lt 64 -or [BitConverter]::ToUInt16($exeBytes, 0) -ne 0x5A4D) {
    throw 'The Windows executable is not a PE file'
}
$peOffset = [BitConverter]::ToInt32($exeBytes, 0x3C)
if ($peOffset -lt 0 -or $peOffset -gt $exeBytes.Length - 94 -or
    [BitConverter]::ToUInt32($exeBytes, $peOffset) -ne 0x00004550 -or
    [BitConverter]::ToUInt16($exeBytes, $peOffset + 4) -ne 0x8664 -or
    [BitConverter]::ToUInt16($exeBytes, $peOffset + 24) -ne 0x20B) {
    throw 'The Windows executable is not x64 PE32+'
}
if ([BitConverter]::ToUInt16($exeBytes, $peOffset + 24 + 68) -ne 2) {
    throw 'The Windows executable must use the GUI subsystem; a console build opens a CMD window'
}
Write-Output "Verified x64 Windows GUI executable: $executablePath"

# Check the actual PE resources. An updated PNG alone cannot change GPUI's
# native Windows icon, and Cargo may otherwise reuse an old resource library.
if (-not ('TinyMdIconResources' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;

public static class TinyMdIconResources
{
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr FindResourceW(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr LockResource(IntPtr resource);

    static byte[] Resource(IntPtr module, int id, int type)
    {
        IntPtr entry = FindResourceW(module, new IntPtr(id), new IntPtr(type));
        if (entry == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        int length = checked((int)SizeofResource(module, entry));
        IntPtr handle = LoadResource(module, entry);
        if (handle == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        IntPtr pointer = LockResource(handle);
        if (pointer == IntPtr.Zero) throw new InvalidDataException("Cannot read icon resource");
        var bytes = new byte[length];
        Marshal.Copy(pointer, bytes, 0, length);
        return bytes;
    }

    public static int Verify(string executable, byte[] ico)
    {
        // Map resources without executing the application or its dependencies.
        IntPtr module = LoadLibraryExW(executable, IntPtr.Zero, 0x22);
        if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try
        {
            byte[] group = Resource(module, 1, 14);
            int count = BitConverter.ToUInt16(ico, 4);
            if (group.Length != 6 + count * 14 || ico.Length < 6 + count * 16)
                throw new InvalidDataException("EXE icon sizes do not match the committed ICO");
            for (int i = 0; i < 6; i++)
                if (group[i] != ico[i])
                    throw new InvalidDataException("EXE icon group header does not match the ICO");
            for (int i = 0; i < count; i++)
            {
                int g = 6 + 14 * i, d = 6 + 16 * i;
                for (int j = 0; j < 12; j++)
                    if (group[g + j] != ico[d + j])
                        throw new InvalidDataException("EXE icon directory does not match the ICO");
                byte[] frame = Resource(module, BitConverter.ToUInt16(group, g + 12), 3);
                int length = checked((int)BitConverter.ToUInt32(ico, d + 8));
                int offset = checked((int)BitConverter.ToUInt32(ico, d + 12));
                if (frame.Length != length || offset < 6 + count * 16 ||
                    offset > ico.Length - length)
                    throw new InvalidDataException("Invalid EXE icon frame length");
                for (int j = 0; j < length; j++)
                    if (frame[j] != ico[offset + j])
                        throw new InvalidDataException("EXE has stale icon pixels; rebuild it");
            }
            return count;
        }
        finally { FreeLibrary(module); }
    }
}
'@
}
$iconPath = Join-Path $PSScriptRoot '../assets/icons/tiny-md.ico'
$iconCount = [TinyMdIconResources]::Verify($executablePath, [IO.File]::ReadAllBytes($iconPath))
Write-Output "Verified $iconCount embedded icon frames: resource ID 1 matches tiny-md.ico byte for byte"
