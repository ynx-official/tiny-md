#!/bin/sh
# Re-export the committed icon source with macOS tools and Python's standard library.
set -eu
cd "$(dirname "$0")/.."
icons="$(pwd)/assets/icons"
work="$(pwd)/target/app-icons"
iconset="$work/tiny-md.iconset"
mkdir -p "$iconset"

for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$icons/tiny-md-source.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    retina=$((size * 2))
    sips -z "$retina" "$retina" "$icons/tiny-md-source.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$icons/tiny-md.icns"
cp "$iconset/icon_512x512@2x.png" "$icons/tiny-md.png"

for size in 16 24 32 48 64 128 256; do
    sips -z "$size" "$size" "$icons/tiny-md-source.png" --out "$work/windows-$size.png" >/dev/null
done
# ICO supports PNG entries; package the resized PNGs without altering their pixels/alpha.
python3 - "$work" "$icons/tiny-md.ico" <<'PY'
import struct
import sys
from pathlib import Path

work, destination = map(Path, sys.argv[1:])
sizes = (16, 24, 32, 48, 64, 128, 256)
entries = [work.joinpath(f"windows-{size}.png").read_bytes() for size in sizes]
offset = 6 + 16 * len(sizes)
directory = bytearray(struct.pack("<HHH", 0, 1, len(sizes)))
for size, data in zip(sizes, entries):
    directory.extend(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset))
    offset += len(data)
destination.write_bytes(directory + b"".join(entries))
PY
printf 'Exported PNG, ICNS and ICO to %s\n' "$icons"
