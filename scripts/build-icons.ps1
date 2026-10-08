#Requires -Version 7.2
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../assets/icons')
)

$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Use build-icons.sh on macOS' }
Add-Type -AssemblyName System.Drawing

# Keep image processing native to Windows; normal checks need no Pillow or SDK.
if (-not ('TinyMdIconExport' -as [type])) {
    $drawingReferences = @('System.Drawing.Common', 'System.Drawing.Primitives', 'System.Collections', 'System.Console')
    # .NET 10 splits the native GDI interfaces into separate assemblies.
    foreach ($assembly in @('System.Private.Windows.GdiPlus', 'System.Private.Windows.Core')) {
        if (Test-Path -LiteralPath (Join-Path $PSHOME "$assembly.dll")) { $drawingReferences += $assembly }
    }
    Add-Type -ReferencedAssemblies $drawingReferences -TypeDefinition @'
using System;
using System.IO;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Collections.Generic;
using System.Text;

public static class TinyMdIconExport
{
    static readonly int[] Sizes = { 16, 20, 24, 32, 40, 48, 64, 128, 256 };
    const double WindowsCanvasFraction = 0.98;

    static Rectangle Artwork(Bitmap source)
    {
        int x0 = source.Width, y0 = source.Height, x1 = -1, y1 = -1;
        int coreX0 = source.Width, coreY0 = source.Height, coreX1 = -1, coreY1 = -1;
        for (int y = 0; y < source.Height; y++)
            for (int x = 0; x < source.Width; x++)
            {
                byte alpha = source.GetPixel(x, y).A;
                // The original cutout has stray pixels with almost zero alpha
                // across its entire canvas. They must not determine its size.
                if (alpha >= 8)
                {
                    x0 = Math.Min(x0, x); y0 = Math.Min(y0, y);
                    x1 = Math.Max(x1, x); y1 = Math.Max(y1, y);
                }
                if (alpha >= 128)
                {
                    coreX0 = Math.Min(coreX0, x); coreY0 = Math.Min(coreY0, y);
                    coreX1 = Math.Max(coreX1, x); coreY1 = Math.Max(coreY1, y);
                }
            }
        if (coreX1 < coreX0) throw new InvalidDataException("Icon has no visible artwork");
        // Center the solid tile, not its asymmetric bottom/right drop shadow.
        int centerX2 = coreX0 + coreX1 + 1, centerY2 = coreY0 + coreY1 + 1;
        int w = Math.Max(centerX2 - 2 * x0, 2 * (x1 + 1) - centerX2);
        int h = Math.Max(centerY2 - 2 * y0, 2 * (y1 + 1) - centerY2);
        return new Rectangle((centerX2 - w) / 2, (centerY2 - h) / 2, w, h);
    }

    static Bitmap Scale(Bitmap source, Rectangle crop, int size, double fraction)
    {
        var frame = new Bitmap(size, size, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(frame))
        using (var attributes = new ImageAttributes())
        {
            g.Clear(Color.Transparent);
            g.CompositingMode = CompositingMode.SourceCopy;
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;
            // Reflect sampling at the image edge to keep full-bleed frames opaque.
            attributes.SetWrapMode(WrapMode.TileFlipXY);
            double scale = size * fraction / Math.Max(crop.Width, crop.Height);
            int w = (int)Math.Round(crop.Width * scale);
            int h = (int)Math.Round(crop.Height * scale);
            g.DrawImage(source, new Rectangle((size - w) / 2, (size - h) / 2, w, h),
                crop.X, crop.Y, crop.Width, crop.Height, GraphicsUnit.Pixel, attributes);
        }
        return frame;
    }

    static byte[] Png(Bitmap frame)
    {
        using (var stream = new MemoryStream())
        {
            frame.Save(stream, ImageFormat.Png);
            return stream.ToArray();
        }
    }

    static byte[] Argb(Bitmap frame)
    {
        using (var stream = new MemoryStream())
        using (var writer = new BinaryWriter(stream))
        {
            writer.Write(Encoding.ASCII.GetBytes("ARGB"));
            for (int channel = 0; channel < 4; channel++)
            {
                var plane = new byte[frame.Width * frame.Height];
                for (int y = 0; y < frame.Height; y++)
                    for (int x = 0; x < frame.Width; x++)
                    {
                        var c = frame.GetPixel(x, y);
                        plane[y * frame.Width + x] = channel == 0 ? c.A :
                            channel == 1 ? c.R : channel == 2 ? c.G : c.B;
                    }
                // Native ICNS RLE literal packets: control byte + up to 128 bytes.
                for (int i = 0; i < plane.Length; i += 128)
                {
                    int length = Math.Min(128, plane.Length - i);
                    writer.Write((byte)(length - 1));
                    writer.Write(plane, i, length);
                }
            }
            return stream.ToArray();
        }
    }

    static void BigEndian(BinaryWriter writer, int value)
    {
        writer.Write(new byte[] { (byte)(value >> 24), (byte)(value >> 16),
            (byte)(value >> 8), (byte)value });
    }

    public static void Export(string windowsPath, string macosPath, string destination)
    {
        Directory.CreateDirectory(destination);
        using (var windows = new Bitmap(windowsPath))
        using (var macos = new Bitmap(macosPath))
        {
            if (macos.Width != macos.Height)
                throw new InvalidDataException("macOS source must be square");
            for (int y = 0; y < macos.Height; y++)
                for (int x = 0; x < macos.Width; x++)
                    if (macos.GetPixel(x, y).A != 255)
                        throw new InvalidDataException("macOS source must be fully opaque");

            var crop = Artwork(windows);
            Console.WriteLine("Windows artwork crop: {0}", crop);
            using (var frame = Scale(windows, crop, 1024, WindowsCanvasFraction))
                File.WriteAllBytes(Path.Combine(destination, "tiny-md.png"), Png(frame));

            var frames = new List<byte[]>();
            foreach (int size in Sizes)
                using (var frame = Scale(windows, crop, size, WindowsCanvasFraction))
                    frames.Add(Png(frame));
            using (var stream = File.Create(Path.Combine(destination, "tiny-md.ico")))
            using (var writer = new BinaryWriter(stream))
            {
                writer.Write((ushort)0); writer.Write((ushort)1);
                writer.Write((ushort)Sizes.Length);
                int offset = 6 + 16 * Sizes.Length;
                for (int i = 0; i < Sizes.Length; i++)
                {
                    writer.Write((byte)(Sizes[i] % 256));
                    writer.Write((byte)(Sizes[i] % 256));
                    writer.Write((ushort)0); writer.Write((ushort)1);
                    writer.Write((ushort)32); writer.Write(frames[i].Length);
                    writer.Write(offset); offset += frames[i].Length;
                }
                foreach (var frame in frames) writer.Write(frame);
            }

            string[] tags = { "ic04", "ic11", "ic05", "ic12", "ic07",
                "ic13", "ic08", "ic14", "ic09", "ic10" };
            int[] dimensions = { 16, 32, 32, 64, 128, 256, 256, 512, 512, 1024 };
            var entries = new List<byte[]>();
            int total = 8;
            for (int i = 0; i < tags.Length; i++)
                using (var frame = Scale(macos, new Rectangle(0, 0, macos.Width,
                    macos.Height), dimensions[i], 1))
                {
                    byte[] payload = tags[i] == "ic04" || tags[i] == "ic05"
                        ? Argb(frame) : Png(frame);
                    entries.Add(payload); total += 8 + payload.Length;
                }
            using (var stream = File.Create(Path.Combine(destination, "tiny-md.icns")))
            using (var writer = new BinaryWriter(stream))
            {
                writer.Write(Encoding.ASCII.GetBytes("icns")); BigEndian(writer, total);
                for (int i = 0; i < entries.Count; i++)
                {
                    writer.Write(Encoding.ASCII.GetBytes(tags[i]));
                    BigEndian(writer, 8 + entries[i].Length); writer.Write(entries[i]);
                }
            }
        }
    }
}
'@
}

$sourceDirectory = Join-Path $PSScriptRoot '../assets/icons'
[TinyMdIconExport]::Export(
    (Join-Path $sourceDirectory 'tiny-md-source.png'),
    (Join-Path $sourceDirectory 'tiny-md-macos-source.png'),
    [IO.Path]::GetFullPath($OutputDirectory)
)
Write-Output "Exported PNG, ICO and ICNS to $([IO.Path]::GetFullPath($OutputDirectory))"
