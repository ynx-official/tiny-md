use crate::{
    Asset, CURRENT_VERSION, REPOSITORY, Release, asset_url, parse_manifest, redirect_policy,
    user_notes, validate_asset, validate_digest, version,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::{collections::HashMap, io::Read, time::Duration};

const LIMIT: u64 = 2 * 1024 * 1024;
pub const MANIFEST_URL: &str =
    "https://github.com/ynx-official/tiny-md/releases/latest/download/update-manifest.json";
const API_URL: &str = "https://api.github.com/repos/ynx-official/tiny-md/releases/latest";
const LATEST_URL: &str = "https://github.com/ynx-official/tiny-md/releases/latest";

pub struct HttpText {
    pub status: u16,
    pub final_url: String,
    pub body: String,
}

/// Injectable only at the network boundary; production always uses HTTPS.
pub trait ReleaseTransport {
    fn get(&self, url: &str) -> Result<HttpText>;
}

struct Http(reqwest::blocking::Client);

#[derive(Debug)]
struct InvalidResponse(&'static str);
impl std::fmt::Display for InvalidResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for InvalidResponse {}

impl ReleaseTransport for Http {
    fn get(&self, url: &str) -> Result<HttpText> {
        let mut request = self.0.get(url);
        if url.starts_with("https://api.github.com/") {
            request = request.header("Accept", "application/vnd.github+json");
        }
        let response = request.send()?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let mut bytes = Vec::new();
        response.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT {
            return Err(InvalidResponse("更新元数据超过大小限制").into());
        }
        let body = String::from_utf8(bytes).map_err(|_| InvalidResponse("更新元数据不是 UTF-8"))?;
        Ok(HttpText {
            status,
            final_url,
            body,
        })
    }
}

fn fallback(response: &Result<HttpText>) -> bool {
    match response {
        Err(error) => !error.is::<InvalidResponse>(),
        Ok(r) => matches!(r.status, 404 | 403 | 429 | 500..=599),
    }
}

fn successful(response: HttpText) -> Result<HttpText> {
    ensure!(
        response.status == 200,
        "更新服务器返回 HTTP {}",
        response.status
    );
    ensure!(
        response.body.len() as u64 <= LIMIT,
        "更新元数据超过大小限制"
    );
    Ok(response)
}

fn notes(transport: &impl ReleaseTransport, release: &mut Release) {
    if let Ok(r) = transport.get(&asset_url(&release.version, "release-notes.md"))
        && let Ok(r) = successful(r)
    {
        release.notes = user_notes(&r.body);
    }
}

pub fn check() -> Result<Release> {
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(redirect_policy())
        .user_agent(format!("tiny-md/{CURRENT_VERSION}"))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()?;
    check_with(&Http(client))
}

/// Network availability permits fallback. Parsed but invalid metadata never does.
pub fn check_with(transport: &impl ReleaseTransport) -> Result<Release> {
    let response = transport.get(MANIFEST_URL);
    if !fallback(&response) {
        let mut release = parse_manifest(&successful(response?)?.body)?;
        notes(transport, &mut release);
        return Ok(release);
    }
    let response = transport.get(API_URL);
    if let Ok(r) = &response
        && r.status == 404
    {
        bail!("尚未发布可用版本");
    }
    if !fallback(&response) {
        let data: Github =
            serde_json::from_str(&successful(response?)?.body).context("GitHub 版本响应无效")?;
        ensure!(
            !data.draft && !data.prerelease && version(&data.tag_name)?.pre.is_empty(),
            "稳定通道拒绝草稿和预发布"
        );
        let sums = if data
            .assets
            .iter()
            .any(|a| a.digest.is_none() && is_payload(&a.name))
        {
            checksums(&successful(transport.get(&asset_url(&data.tag_name, "SHA256SUMS"))?)?.body)?
        } else {
            HashMap::new()
        };
        let mut assets = Vec::new();
        for a in data.assets.into_iter().filter(|a| is_payload(&a.name)) {
            let digest = a
                .digest
                .or_else(|| sums.get(&a.name).cloned())
                .context("GitHub 附件没有校验和")?;
            let asset = Asset {
                name: a.name,
                url: a.browser_download_url,
                size: a.size,
                digest,
            };
            validate_asset(&data.tag_name, &asset, false)?;
            ensure!(
                !assets.iter().any(|old: &Asset| old.name == asset.name),
                "GitHub 附件重复"
            );
            assets.push(asset);
        }
        ensure!(!assets.is_empty(), "GitHub 版本没有安装附件");
        return Ok(Release {
            version: data.tag_name,
            notes: user_notes(data.body.as_deref().unwrap_or_default()),
            assets,
        });
    }
    // Resolve the tag from the official page, then use machine-readable assets.
    // HTML presentation changes must not cause guessed download URLs or hashes.
    let page = successful(transport.get(LATEST_URL)?)?;
    let prefix = format!("https://github.com/{REPOSITORY}/releases/tag/");
    let tag = page
        .final_url
        .strip_prefix(&prefix)
        .context("发布页面未返回本仓库版本标签")?
        .replace("%2B", "+");
    ensure!(
        !tag.contains(['/', '?', '#']) && version(&tag)?.pre.is_empty(),
        "发布页面标签无效"
    );
    let response = transport.get(&asset_url(&tag, "update-manifest.json"));
    if !fallback(&response) {
        let mut release = parse_manifest(&successful(response?)?.body)?;
        ensure!(release.version == tag, "发布页面与清单版本不一致");
        notes(transport, &mut release);
        return Ok(release);
    }
    let sums = checksums(&successful(transport.get(&asset_url(&tag, "SHA256SUMS"))?)?.body)?;
    let mut assets = Vec::new();
    for (name, digest) in sums.into_iter().filter(|(n, _)| is_payload(n)) {
        let asset = Asset {
            url: asset_url(&tag, &name),
            name,
            digest,
            size: 0,
        };
        validate_asset(&tag, &asset, true)?;
        assets.push(asset);
    }
    ensure!(!assets.is_empty(), "发布页面兜底没有带校验和的附件");
    let mut release = Release {
        version: tag,
        notes: String::new(),
        assets,
    };
    notes(transport, &mut release);
    Ok(release)
}

fn is_payload(name: &str) -> bool {
    name.ends_with(".exe") || name.ends_with(".zip") || name.ends_with(".dmg")
}

fn checksums(text: &str) -> Result<HashMap<String, String>> {
    let mut result = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (hash, name) = line
            .split_once(char::is_whitespace)
            .context("SHA256SUMS 格式错误")?;
        let name = name
            .trim()
            .trim_start_matches('*')
            .strip_prefix("./")
            .unwrap_or(name.trim().trim_start_matches('*'));
        ensure!(!name.contains(['/', '\\']), "校验和附件名无效");
        let digest = format!("sha256:{hash}");
        validate_digest(&digest)?;
        ensure!(
            result.insert(name.into(), digest).is_none(),
            "校验和条目重复"
        );
    }
    Ok(result)
}

#[derive(Deserialize)]
struct Github {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}
#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}
