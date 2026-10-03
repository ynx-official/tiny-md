#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
profile=debug
launch=false
version=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --release) profile=release ;;
        --launch) launch=true ;;
        --version)
            if [ "$#" -lt 2 ]; then
                printf '%s\n' '--version requires a version such as 0.1.0' >&2
                exit 1
            fi
            version="$2"
            shift
            ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; exit 1 ;;
    esac
    shift
done
if [ -z "$version" ]; then
    version=$(cargo pkgid -p tiny-md | sed 's/.*[@#]//')
fi
if ! printf '%s\n' "$version" | LC_ALL=C grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    printf 'Invalid bundle version: %s (expected X.Y.Z)\n' "$version" >&2
    exit 1
fi
if [ "$profile" = release ]; then
    cargo build --locked --release -p tiny-md
    bundle="$(pwd)/target/release/Tiny MD.app"
else
    cargo build --locked -p tiny-md
    bundle="$(pwd)/target/Tiny MD.app"
fi
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "target/$profile/tiny-md" "$bundle/Contents/MacOS/tiny-md"
cp assets/icons/tiny-md.icns "$bundle/Contents/Resources/tiny-md.icns"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Tiny MD</string>
<key>CFBundleDisplayName</key><string>Tiny MD</string>
<key>CFBundleIdentifier</key><string>dev.tiny-md.app</string>
<key>CFBundleExecutable</key><string>tiny-md</string>
<key>CFBundleIconFile</key><string>tiny-md.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
PLIST
printf 'Built: %s\n' "$bundle"
if [ "$launch" = true ]; then open "$bundle"; fi
