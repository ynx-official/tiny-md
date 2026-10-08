use tiny_md_updater::{is_newer, parse_manifest};

fn manifest() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "version": "v0.2.0",
        "notes_url": "https://github.com/ynx-official/tiny-md/releases/download/v0.2.0/release-notes.md",
        "assets": [{
            "name": "tiny-md-v0.2.0-windows-x64-setup.exe",
            "url": "https://github.com/ynx-official/tiny-md/releases/download/v0.2.0/tiny-md-v0.2.0-windows-x64-setup.exe",
            "size": 3,
            "digest": "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        }]
    })
}

struct FakeSource {
    calls: std::cell::RefCell<Vec<String>>,
    manifest: Option<String>,
    api: Option<String>,
}
impl tiny_md_updater::ReleaseTransport for FakeSource {
    fn get(&self, url: &str) -> anyhow::Result<tiny_md_updater::HttpText> {
        self.calls.borrow_mut().push(url.into());
        let (status, body, final_url) = if url.ends_with("/latest/download/update-manifest.json") {
            (
                if self.manifest.is_some() { 200 } else { 404 },
                self.manifest.clone().unwrap_or_default(),
                url.into(),
            )
        } else if url.starts_with("https://api.github.com") {
            (
                if self.api.is_some() { 200 } else { 429 },
                self.api.clone().unwrap_or_default(),
                url.into(),
            )
        } else if url.ends_with("/latest") {
            (
                200,
                String::new(),
                "https://github.com/ynx-official/tiny-md/releases/tag/v0.2.0".into(),
            )
        } else if url.ends_with("/v0.2.0/update-manifest.json") {
            (200, manifest().to_string(), url.into())
        } else if url.ends_with("/release-notes.md") {
            (
                200,
                "## 改进\n\n- 在线更新。\n\n## 验证结果\n\n内部审计。".into(),
                url.into(),
            )
        } else {
            (404, String::new(), url.into())
        };
        Ok(tiny_md_updater::HttpText {
            status,
            body,
            final_url,
        })
    }
}

#[test]
fn rate_limited_api_uses_release_page_and_matching_manifest() {
    let source = FakeSource {
        calls: Default::default(),
        manifest: None,
        api: None,
    };
    let release = tiny_md_updater::check_with(&source).unwrap();
    assert_eq!(release.version, "v0.2.0");
    assert!(release.notes.contains("在线更新"));
    assert!(!release.notes.contains("内部审计"));
    assert!(
        source
            .calls
            .borrow()
            .iter()
            .any(|url| url.ends_with("/releases/latest"))
    );
}

#[test]
fn malformed_manifest_is_terminal_and_does_not_weaken_to_api() {
    let source = FakeSource {
        calls: Default::default(),
        manifest: Some("invalid".into()),
        api: None,
    };
    assert!(tiny_md_updater::check_with(&source).is_err());
    assert_eq!(source.calls.borrow().len(), 1);
}

#[test]
fn api_rejects_drafts_and_previews() {
    for (draft, prerelease) in [(true, false), (false, true)] {
        let source = FakeSource {calls: Default::default(), manifest: None, api: Some(serde_json::json!({
            "tag_name": "v0.2.0", "draft": draft, "prerelease": prerelease, "body": "", "assets": []
        }).to_string())};
        assert!(tiny_md_updater::check_with(&source).is_err());
        assert_eq!(source.calls.borrow().len(), 2);
    }
}

#[test]
fn platform_and_installation_edition_never_cross_select() {
    let release = parse_manifest(&manifest().to_string()).unwrap();
    assert!(
        release
            .asset(tiny_md_updater::Installation::WindowsInstalled)
            .is_ok()
    );
    assert!(
        release
            .asset(tiny_md_updater::Installation::WindowsPortable)
            .is_err()
    );
    assert!(
        release
            .asset(tiny_md_updater::Installation::MacApp)
            .is_err()
    );
}

#[test]
fn numeric_versions_and_prereleases_follow_stable_channel_policy() {
    assert!(is_newer("0.1.9", "v0.1.10").unwrap());
    assert!(is_newer("0.2.0-beta.1", "v0.2.0").unwrap());
    assert!(!is_newer("0.2.0", "v0.2.0-beta.1").unwrap());
    assert!(!is_newer("0.2.0+build.2", "v0.2.0+build.3").unwrap());
    assert!(!is_newer("0.3.0", "v0.2.0").unwrap());
    assert!(is_newer("0.2.0", "v01.2.0").is_err());
}

#[test]
fn accepts_manifest_with_exact_repository_and_digest() {
    let release = parse_manifest(&manifest().to_string()).unwrap();
    assert_eq!(release.version, "v0.2.0");
    assert_eq!(release.assets[0].size, 3);
}

#[test]
fn rejects_schema_duplicate_assets_and_unsafe_metadata() {
    for key in ["url", "digest", "name", "size"] {
        let mut value = manifest();
        value["assets"][0][key] = match key {
            "url" => serde_json::json!(
                "https://github.com/attacker/repo/releases/download/v0.2.0/payload.exe"
            ),
            "digest" => serde_json::json!("sha256:abc"),
            "name" => serde_json::json!("../tiny-md.exe"),
            _ => serde_json::json!(0),
        };
        assert!(parse_manifest(&value.to_string()).is_err(), "{key}");
    }
    let mut value = manifest();
    value["schema_version"] = 2.into();
    assert!(parse_manifest(&value.to_string()).is_err());
    let mut value = manifest();
    let duplicate = value["assets"][0].clone();
    value["assets"].as_array_mut().unwrap().push(duplicate);
    assert!(parse_manifest(&value.to_string()).is_err());
    let mut value = manifest();
    value["notes_url"] = "http://example.com/notes".into();
    assert!(parse_manifest(&value.to_string()).is_err());
}
