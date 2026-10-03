use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/tiny-md.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../assets/icons/tiny-md.ico");
    let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("tiny-md.rc");
    // Forward slashes work with both MSVC rc.exe and GNU windres.
    let icon = icon
        .to_str()
        .expect("icon path must be UTF-8")
        .replace('\\', "/");
    // GPUI loads the application's Windows icon from resource ID 1.
    fs::write(
        &resource,
        format!("#pragma code_page(65001)\n1 ICON \"{icon}\"\n"),
    )
    .expect("failed to write Windows icon resource");
    embed_resource::compile_for(&resource, ["tiny-md"], embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed Windows app icon; install a Windows resource compiler");
}
