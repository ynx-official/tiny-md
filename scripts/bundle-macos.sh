#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked -p tiny-md
bundle="$(pwd)/target/Tiny MD.app"
mkdir -p "$bundle/Contents/MacOS"
cp target/debug/tiny-md "$bundle/Contents/MacOS/tiny-md"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Tiny MD</string>
<key>CFBundleDisplayName</key><string>Tiny MD</string>
<key>CFBundleIdentifier</key><string>dev.tiny-md.app</string>
<key>CFBundleExecutable</key><string>tiny-md</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
PLIST
printf 'Built: %s\n' "$bundle"
if [ "${1:-}" = "--launch" ]; then open "$bundle"; fi
