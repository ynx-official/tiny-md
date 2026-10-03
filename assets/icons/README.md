# Tiny MD icon

- `tiny-md-source.png`: original generated master, with transparent margins.
- `tiny-md.png`: 1024 × 1024 PNG.
- `tiny-md.icns`: macOS standard and Retina sizes, through 1024 × 1024.
- `tiny-md.ico`: Windows 16, 24, 32, 48, 64, 128 and 256 px PNG entries.

Generated with the built-in ImageGen tool from the approved T icon preview. Conversion
uses `sips`, `iconutil`, and Python's standard library to assemble the ICO container.
Run `sh scripts/build-icons.sh` on macOS to regenerate the exported files.

Design prompt: extract the approved macOS icon into one square standalone asset;
preserve its white rounded tile, dark charcoal serif uppercase T, pale grey rounded
text lines, restrained depth and soft shadow. Center the tile with balanced
transparent margins. Keep the tile opaque and its surroundings transparent.
No labels, comparison board, background fill, green, M, folded page or extra badges.

macOS integration: `scripts/bundle-macos.sh` copies the ICNS into the application
bundle's Resources directory and declares it in Info.plist. The macOS startup hook
also sets the Dock icon from embedded ICNS bytes, including during `cargo run`;
it does not depend on the working directory or a bundled application. Windows integration:
`crates/app/build.rs` compiles and links an icon resource into `tiny-md.exe` when
targeting Windows. Windows runtime verification remains separate from icon export.
