//! Read-only network smoke test; downloads are verified and discarded, never run.
use tiny_md_updater::{
    Cancellation, Installation, Preferences, download_routed,
    routes::{RouteMode, check_routed, probe_routes},
};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let mode = if args.iter().any(|a| a == "--github") {
        RouteMode::Github
    } else if args.iter().any(|a| a == "--gh-proxy") {
        RouteMode::GhProxy
    } else if args.iter().any(|a| a == "--ghfast") {
        RouteMode::GhFast
    } else {
        RouteMode::Auto
    };
    let prefs = Preferences {
        route_mode: mode,
        ..Default::default()
    };
    let report = if args.iter().any(|a| a == "--probe-only") {
        probe_routes(&prefs)?
    } else {
        check_routed(&prefs, |_| {})?
    };
    for entry in &report.entries {
        println!(
            "{}: {:?} ms, {}",
            entry.route.label(),
            entry.elapsed_ms,
            entry.state.label()
        );
    }
    let release = report
        .release
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(report.error.unwrap_or_else(|| "没有可用线路".into())))?;
    println!("Verified release: {}", release.version);
    if args
        .iter()
        .any(|a| a == "--download" || a == "--cancel-download")
    {
        let cancel = Cancellation::default();
        let cancelling = args.iter().any(|a| a == "--cancel-download");
        let result = download_routed(
            release,
            Installation::WindowsInstalled,
            &report.routes,
            &cancel,
            |done, _| {
                if cancelling && done > 0 {
                    cancel.cancel();
                }
            },
            |route| println!("Download route: {}", route.label()),
        );
        if cancelling {
            anyhow::ensure!(cancel.is_cancelled() && result.is_err(), "取消下载未生效");
            println!("Download cancellation verified");
        } else {
            let update = result?;
            println!(
                "Verified {} ({} bytes); installer not executed",
                update.asset.name, update.asset.size
            );
        }
    }
    Ok(())
}
