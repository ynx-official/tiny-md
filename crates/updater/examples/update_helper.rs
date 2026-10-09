//! Headless helper driver for isolated installer failure tests. The application
//! adds native failure feedback; this driver returns errors to the test runner.
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--version") {
        println!("Tiny MD {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.first().is_some_and(|arg| arg == "--apply-update")
        && let Some(path) = args.get(1)
    {
        if let Err(error) = tiny_md_updater::install::helper_main(std::path::Path::new(path)) {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return;
    }
    eprintln!("Use --version or --apply-update <isolated plan>");
    std::process::exit(2);
}
