#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
version="${1:-}"
bundle="${2:-target/release/Tiny MD.app}"
if ! printf '%s\n' "$version" | LC_ALL=C grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.+-]+)?$'; then
    printf '%s\n' 'Usage: sh scripts/package-macos.sh X.Y.Z [path/to/Tiny MD.app]' >&2
    exit 1
fi
test -x "$bundle/Contents/MacOS/tiny-md"
plutil -lint "$bundle/Contents/Info.plist"
stage=$(mktemp -d "${TMPDIR:-/tmp}/tiny-md-dmg.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
ditto "$bundle" "$stage/Tiny MD.app"
codesign --force --deep --sign - "$stage/Tiny MD.app"
codesign --verify --deep --strict "$stage/Tiny MD.app"
ln -s /Applications "$stage/Applications"
mkdir -p target/release-assets
image="target/release-assets/tiny-md-v${version}-macos-arm64.dmg"
hdiutil create -volname "Tiny MD $version" -srcfolder "$stage" -ov -format UDZO "$image"
hdiutil verify "$image"
archive="target/release-assets/tiny-md-v${version}-macos-arm64-portable.zip"
ditto -c -k --norsrc --keepParent "$stage/Tiny MD.app" "$archive"
unzip -t "$archive"
printf 'Built: %s\n' "$image"
printf 'Built update archive: %s\n' "$archive"
