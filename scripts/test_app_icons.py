"""Check shipped icon pixels using only the standard library on either platform."""

import struct
import unittest
import zlib
from pathlib import Path


ICONS = Path(__file__).resolve().parents[1] / "assets" / "icons"
WINDOWS_SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)


def read_png(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("icon frame is not PNG")
    offset, compressed = 8, bytearray()
    while offset < len(data):
        length, tag = struct.unpack_from(">I4s", data, offset)
        payload = data[offset + 8:offset + 8 + length]
        if len(payload) != length:
            raise ValueError("truncated PNG")
        if tag == b"IHDR":
            width, height, depth, color, compression, filtering, interlace = struct.unpack(
                ">IIBBBBB", payload
            )
            if depth != 8 or color not in (2, 6) or compression or filtering or interlace:
                raise ValueError("icons must be non-interlaced 8-bit RGB/RGBA PNGs")
        elif tag == b"IDAT":
            compressed.extend(payload)
        offset += length + 12
    channels = 4 if color == 6 else 3
    stride = width * channels
    raw = zlib.decompress(compressed)
    if len(raw) != height * (stride + 1):
        raise ValueError("incorrect PNG scanline length")
    previous, pixels = bytearray(stride), bytearray()
    for y in range(height):
        start = y * (stride + 1)
        method = raw[start]
        row = bytearray(raw[start + 1:start + 1 + stride])
        if method not in range(5):
            raise ValueError("unknown PNG filter")
        if method:
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                above = previous[i]
                upper_left = previous[i - channels] if i >= channels else 0
                if method == 1:
                    predictor = left
                elif method == 2:
                    predictor = above
                elif method == 3:
                    predictor = (left + above) // 2
                else:
                    p = left + above - upper_left
                    distances = (abs(p - left), abs(p - above), abs(p - upper_left))
                    predictor = (left, above, upper_left)[distances.index(min(distances))]
                row[i] = (row[i] + predictor) & 255
        previous = row
        if channels == 4:
            pixels.extend(row)
        else:
            for i in range(0, stride, 3):
                pixels.extend(row[i:i + 3])
                pixels.append(255)
    return width, height, pixels


def read_icns():
    data = (ICONS / "tiny-md.icns").read_bytes()
    if data[:4] != b"icns" or struct.unpack_from(">I", data, 4)[0] != len(data):
        raise ValueError("invalid ICNS header")
    offset, entries = 8, {}
    while offset < len(data):
        tag, length = struct.unpack_from(">4sI", data, offset)
        if length < 8 or offset + length > len(data) or tag in entries:
            raise ValueError("invalid ICNS entry")
        entries[tag] = data[offset + 8:offset + length]
        offset += length
    return entries


def read_argb(data, size):
    if not data.startswith(b"ARGB"):
        raise ValueError("small ICNS frame must use native ARGB")
    offset, planes = 4, []
    for _ in range(4):
        plane = bytearray()
        while len(plane) < size * size:
            control = data[offset]
            offset += 1
            if control < 128:
                count = control + 1
                plane.extend(data[offset:offset + count])
                offset += count
            else:
                plane.extend([data[offset]] * (control - 125))
                offset += 1
            if len(plane) > size * size:
                raise ValueError("ARGB plane overflow")
        planes.append(plane)
    if offset != len(data):
        raise ValueError("unexpected trailing ARGB bytes")
    pixels = bytearray()
    for i in range(size * size):
        pixels.extend(plane[i] for plane in (planes[1], planes[2], planes[3], planes[0]))
    return pixels


class AppIconTests(unittest.TestCase):
    def assert_windows_artwork(self, image):
        width, height, pixels = image
        self.assertEqual(width, height)
        xs, ys = [], []
        for index, alpha in enumerate(pixels[3::4]):
            if alpha >= 128:
                xs.append(index % width)
                ys.append(index // width)
        self.assertTrue(xs, "icon has no visible artwork")
        for extent in (max(xs) - min(xs) + 1, max(ys) - min(ys) + 1):
            self.assertGreaterEqual(
                extent * 8, width * 7,
                f"{width}px icon has only {extent}px of visible artwork; excessive padding",
            )
        self.assertLessEqual(abs(min(xs) + max(xs) - (width - 1)), 2)
        self.assertLessEqual(abs(min(ys) + max(ys) - (height - 1)), 2)
        for index in (0, width - 1, width * (height - 1), width * height - 1):
            self.assertEqual(pixels[index * 4 + 3], 0, "Windows corners must be transparent")

    def test_window_png_uses_available_canvas(self):
        image = read_png((ICONS / "tiny-md.png").read_bytes())
        self.assertEqual(image[:2], (1024, 1024))
        self.assert_windows_artwork(image)

    def test_windows_ico_covers_dpi_sizes_without_excessive_padding(self):
        data = (ICONS / "tiny-md.ico").read_bytes()
        reserved, kind, count = struct.unpack_from("<HHH", data)
        self.assertEqual((reserved, kind), (0, 1))
        sizes = []
        for index in range(count):
            w, h, colors, reserved, planes, depth, length, offset = struct.unpack_from(
                "<BBBBHHII", data, 6 + 16 * index
            )
            size = w or 256
            with self.subTest(size=size):
                self.assertEqual(w, h)
                self.assertEqual((colors, reserved, planes, depth), (0, 0, 1, 32))
                self.assertGreaterEqual(offset, 6 + 16 * count)
                self.assertLessEqual(offset + length, len(data))
                image = read_png(data[offset:offset + length])
                self.assertEqual(image[:2], (size, size))
                self.assert_windows_artwork(image)
            sizes.append(size)
        self.assertEqual(tuple(sizes), WINDOWS_SIZES)

    def test_macos_frames_fill_canvas_for_system_mask(self):
        entries = read_icns()
        for tag, size in ((b"ic11", 32), (b"ic12", 64), (b"ic07", 128),
                          (b"ic13", 256), (b"ic08", 256), (b"ic14", 512),
                          (b"ic09", 512), (b"ic10", 1024)):
            with self.subTest(tag=tag):
                width, height, pixels = read_png(entries[tag])
                self.assertEqual((width, height), (size, size))
                self.assertTrue(all(a == 255 for a in pixels[3::4]),
                                "macOS has an inset transparent tile")
                for i in (0, size - 1, size * (size - 1), size * size - 1):
                    self.assertTrue(all(c >= 235 for c in pixels[i * 4:i * 4 + 3]),
                                    "macOS corners must retain the light paper background")

    def test_macos_small_frames_remain_native_argb(self):
        entries = read_icns()
        for tag, size in ((b"ic04", 16), (b"ic05", 32)):
            pixels = read_argb(entries[tag], size)
            self.assertEqual(len(pixels), size * size * 4)
            if size == 32:
                self.assertEqual(pixels, read_png(entries[b"ic11"])[2])


if __name__ == "__main__":
    unittest.main()
