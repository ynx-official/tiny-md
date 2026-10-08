#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 -B -m unittest discover -s scripts -p 'test_*.py'
python3 -B scripts/release_notes.py --check-current
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
