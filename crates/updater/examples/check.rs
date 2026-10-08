fn main() -> anyhow::Result<()> {
    let release = tiny_md_updater::check()?;
    println!(
        "Latest stable release: {} ({} validated assets)",
        release.version,
        release.assets.len()
    );
    if std::env::args().any(|arg| arg == "--download" || arg == "--cancel-download") {
        let cancel = tiny_md_updater::Cancellation::default();
        let cancelling = std::env::args().any(|arg| arg == "--cancel-download");
        // A smoke probe verifies the existing installer and drops its temporary
        // files. It never executes the downloaded payload or changes installation.
        let result = tiny_md_updater::download(
            &release,
            tiny_md_updater::Installation::WindowsInstalled,
            &cancel,
            |_, _| {
                if cancelling {
                    cancel.cancel();
                }
            },
        );
        if cancelling {
            anyhow::ensure!(
                result.is_err() && cancel.is_cancelled(),
                "cancellation did not interrupt the download"
            );
            println!("Download cancellation verified");
        } else {
            let prepared = result?;
            println!("Verified download: {}", prepared.asset.name);
        }
    }
    for asset in release.assets {
        println!("{} · {} bytes", asset.name, asset.size);
    }
    Ok(())
}
