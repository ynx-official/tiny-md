#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 -B -m unittest discover -s scripts -p 'test_*.py'
if [ "$(uname -s)" = Darwin ]; then
    icon_work="$(mktemp -d "${TMPDIR:-/tmp}/tiny-md-icons.XXXXXX")"
    trap 'rm -rf "$icon_work"' EXIT HUP INT TERM
    iconutil -c iconset assets/icons/tiny-md.icns -o "$icon_work/tiny-md.iconset"
fi
python3 -B scripts/release_notes.py --check-current
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
