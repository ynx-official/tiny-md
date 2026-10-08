use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons/tiny-md.ico");
    println!("cargo:rerun-if-changed=build.rs");

    let version = env::var("CARGO_PKG_VERSION").expect("Cargo provides the package version");
    let notes_path = format!("../../docs/06-delivery/versions/v{version}.md");
    println!("cargo:rerun-if-changed={notes_path}");
    let detail = fs::read_to_string(&notes_path).unwrap_or_else(|_| {
        format!(
            "# Tiny MD v{version}\n\n开发版本；发布日志尚未建立，请查看 CHANGELOG 的 Unreleased。"
        )
    });
    let mut skipping = false;
    let notes = detail
        .lines()
        .filter(|line| {
            if let Some(heading) = line.strip_prefix("## ") {
                skipping = matches!(heading.trim(), "验证结果" | "变更依据");
            }
            !skipping
                && !["[返回版本总览]", "> 状态", "> 最后更新", "> 关联文档"]
                    .iter()
                    .any(|prefix| line.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("release-notes.md"), format!("{}\n", notes.trim()))
        .expect("failed to embed release notes");

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
        {
            let core = version.split(['-', '+']).next().unwrap().replace('.', ",");
            format!("#pragma code_page(65001)\n1 ICON \"{icon}\"\n1 VERSIONINFO\nFILEVERSION {core},0\nPRODUCTVERSION {core},0\nFILEOS 0x40004\nFILETYPE 1\nBEGIN\nBLOCK \"StringFileInfo\"\nBEGIN\nBLOCK \"040904b0\"\nBEGIN\nVALUE \"FileDescription\", \"Tiny MD\\0\"\nVALUE \"ProductName\", \"Tiny MD\\0\"\nVALUE \"FileVersion\", \"{version}\\0\"\nVALUE \"ProductVersion\", \"{version}\\0\"\nEND\nEND\nBLOCK \"VarFileInfo\"\nBEGIN\nVALUE \"Translation\", 0x409, 1200\nEND\nEND\n")
        },
    )
    .expect("failed to write Windows icon resource");
    embed_resource::compile_for(&resource, ["tiny-md"], embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed Windows app icon; install a Windows resource compiler");
}
