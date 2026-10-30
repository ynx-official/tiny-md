use super::*;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::sync::{Barrier, Mutex};

#[test]
fn every_saved_custom_route_is_probed_with_bounded_concurrency() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Measured {
        active: AtomicUsize,
        peak: AtomicUsize,
    }
    impl ProbeTransport for Measured {
        fn get(&self, _: &Route, url: &str, _: Duration) -> Result<crate::HttpText> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(15));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(crate::HttpText {
                status: if url.ends_with(".sig") { 404 } else { 200 },
                final_url: url.into(),
                body: manifest("v1.0.0", &"ab".repeat(32)),
            })
        }
    }
    let transport = Measured {
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    };
    let prefs: Preferences = serde_json::from_value(serde_json::json!({
        "custom_routes": (0..9).map(|n| format!("https://mirror.example/{n}/")).collect::<Vec<_>>()
    }))
    .unwrap();
    let report = probe_with(&prefs, &transport, signing::PUBLIC_KEY).unwrap();
    assert_eq!(report.entries.len(), 12);
    assert_eq!(report.routes.len(), 12);
    assert!((2..=4).contains(&transport.peak.load(Ordering::SeqCst)));
    let prefixes: std::collections::HashSet<_> = report.routes.iter().map(|r| &r.prefix).collect();
    assert_eq!(prefixes.len(), 12);
}

fn manifest(tag: &str, hash: &str) -> String {
    let name = format!("tiny-md-{tag}-windows-x64-setup.exe");
    serde_json::json!({"schema_version":1,"version":tag,"notes_url":asset_url(tag,"release-notes.md"),"assets":[{"name":name,"url":asset_url(tag,&name),"size":3,"digest":format!("sha256:{hash}")}]}).to_string()
}
fn key() -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap()
}
struct Fake {
    bodies: [Option<String>; 3],
    signed: [bool; 3],
    corrupt: bool,
    barrier: Option<Barrier>,
    calls: Mutex<Vec<(RouteMode, String)>>,
}
impl Fake {
    fn same() -> Self {
        Self {
            bodies: std::array::from_fn(|_| Some(manifest("v1.0.0", &"ab".repeat(32)))),
            signed: [false; 3],
            corrupt: false,
            barrier: None,
            calls: Mutex::new(Vec::new()),
        }
    }
    fn run(&self, prefs: &Preferences) -> Result<ProbeReport> {
        probe_with(
            prefs,
            self,
            &signing::encode_hex(key().public_key().as_ref()),
        )
    }
}
impl ProbeTransport for Fake {
    fn get(&self, route: &Route, canonical: &str, remaining: Duration) -> Result<crate::HttpText> {
        assert!(remaining <= PROBE_TIMEOUT && !remaining.is_zero());
        self.calls
            .lock()
            .unwrap()
            .push((route.mode, canonical.into()));
        let index = match route.mode {
            RouteMode::Github => 0,
            RouteMode::GhProxy => 1,
            _ => 2,
        };
        if !canonical.ends_with(".sig")
            && let Some(barrier) = &self.barrier
        {
            barrier.wait();
        }
        if canonical.ends_with(".sig") {
            let signature = if self.corrupt {
                "00".repeat(64)
            } else {
                signing::encode_hex(
                    key()
                        .sign(self.bodies[index].as_ref().unwrap().as_bytes())
                        .as_ref(),
                )
            };
            return Ok(crate::HttpText {
                status: if self.signed[index] { 200 } else { 404 },
                final_url: canonical.into(),
                body: signature,
            });
        }
        if index == 0 {
            std::thread::sleep(Duration::from_millis(25));
        }
        Ok(crate::HttpText {
            status: if self.bodies[index].is_some() {
                200
            } else {
                503
            },
            final_url: canonical.into(),
            body: self.bodies[index].clone().unwrap_or_default(),
        })
    }
}

#[test]
fn legacy_unsigned_routes_are_parallel_and_corroborated_against_official_assets() {
    let mut fake = Fake::same();
    fake.barrier = Some(Barrier::new(3));
    let report = fake.run(&Preferences::default()).unwrap();
    assert_eq!(report.routes.len(), 3);
    assert_ne!(report.routes[0].mode, RouteMode::Github);
    assert!(
        report
            .entries
            .iter()
            .all(|e| e.state == ProbeState::Available)
    );
}

#[test]
fn unsigned_proxy_cannot_become_authority_when_github_is_down() {
    let mut fake = Fake::same();
    fake.bodies[0] = None;
    let report = fake.run(&Preferences::default()).unwrap();
    assert!(report.release.is_none());
    assert!(report.routes.is_empty());
    assert_eq!(report.entries[1].state, ProbeState::Unverified);
}

#[test]
fn signed_proxy_works_without_github_and_rejects_faster_old_versions() {
    let mut fake = Fake::same();
    fake.bodies[0] = None;
    fake.bodies[2] = Some(manifest("v1.1.0", &"cd".repeat(32)));
    fake.signed = [false, true, true];
    let report = fake.run(&Preferences::default()).unwrap();
    assert_eq!(report.release.unwrap().version, "v1.1.0");
    assert_eq!(report.routes.len(), 1);
    assert_eq!(report.routes[0].mode, RouteMode::GhFast);
    assert_eq!(report.entries[1].state, ProbeState::Stale);
}

#[test]
fn invalid_signature_does_not_downgrade_to_unsigned_comparison() {
    let mut fake = Fake::same();
    fake.signed[1] = true;
    fake.corrupt = true;
    let report = fake.run(&Preferences::default()).unwrap();
    assert_eq!(report.entries[1].state, ProbeState::Invalid);
    assert!(!report.routes.iter().any(|r| r.mode == RouteMode::GhProxy));
}

#[test]
fn unsigned_mismatched_assets_and_html_cannot_be_download_routes() {
    let mut fake = Fake::same();
    fake.bodies[1] = Some(manifest("v1.0.0", &"ff".repeat(32)));
    fake.bodies[2] = Some("<html>captcha</html>".into());
    let report = fake.run(&Preferences::default()).unwrap();
    assert_eq!(report.routes, vec![Route::github()]);
    assert_eq!(report.entries[1].state, ProbeState::Unverified);
    assert_eq!(report.entries[2].state, ProbeState::Invalid);
}

#[test]
fn authenticated_conflicting_metadata_is_terminal() {
    let mut fake = Fake::same();
    fake.bodies[1] = Some(manifest("v1.0.0", &"ff".repeat(32)));
    fake.signed[1] = true;
    assert!(fake.run(&Preferences::default()).is_err());
}

#[test]
fn github_only_never_contacts_proxies_and_fixed_mode_never_selects_another_download_route() {
    let fake = Fake::same();
    let report = fake
        .run(&Preferences {
            route_mode: RouteMode::Github,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.routes, vec![Route::github()]);
    assert!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .all(|(mode, _)| *mode == RouteMode::Github)
    );
    let report = fake
        .run(&Preferences {
            route_mode: RouteMode::GhFast,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.routes[0].mode, RouteMode::GhFast);
}

#[test]
fn official_fallback_can_authenticate_an_already_probed_proxy_without_changing_fixed_mode() {
    let mut fake = Fake::same();
    fake.bodies[0] = None;
    let prefs = Preferences {
        route_mode: RouteMode::GhFast,
        ..Default::default()
    };
    let mut report = fake.run(&prefs).unwrap();
    assert!(report.routes.is_empty());
    report.corroborate(parse_manifest(fake.bodies[2].as_ref().unwrap()).unwrap());
    assert_eq!(report.routes.len(), 1);
    assert_eq!(report.routes[0].mode, RouteMode::GhFast);
    assert!(report.release.is_some());
}

#[test]
fn custom_prefix_and_rewrites_preserve_the_canonical_repository_boundary() {
    assert_eq!(
        normalize_custom(" https://mirror.example/prefix ").unwrap(),
        "https://mirror.example/prefix/"
    );
    for bad in [
        "http://mirror.example/",
        "https://user:pass@mirror.example/",
        "https://mirror.example/?token=a",
        "https://mirror.example/#fragment",
        "https://localhost/",
        "https://127.0.0.1/",
        "https://10.0.0.1/",
        "https://[::1]/",
        "https://mirror.example/https://github.com/",
    ] {
        assert!(normalize_custom(bad).is_err(), "{bad}");
    }
    let route = Route {
        mode: RouteMode::Custom,
        prefix: "https://mirror.example/proxy/".into(),
    };
    let canonical = asset_url("v1.0.0", "update-manifest.json");
    assert_eq!(
        route.url(&canonical).unwrap(),
        format!("https://mirror.example/proxy/{canonical}")
    );
    assert!(
        route
            .url("https://github.com/attacker/repo/releases/latest")
            .is_err()
    );
    assert!(
        route
            .url("http://github.com/ynx-official/tiny-md/releases/latest")
            .is_err()
    );
}
