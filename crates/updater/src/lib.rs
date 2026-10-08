use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;

mod download;
pub mod install;
mod preferences;
mod source;
pub use download::{Cancellation, PreparedUpdate, download, verify_file};
pub use preferences::{CheckMode, Preferences, preferences_path};
pub use source::{HttpText, ReleaseTransport, check, check_with};

pub const REPOSITORY: &str = "ynx-official/tiny-md";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_DOWNLOAD: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    pub assets: Vec<Asset>,
}

pub fn version(text: &str) -> Result<Version> {
    Version::parse(text.strip_prefix('v').unwrap_or(text)).context("发布版本号无效")
}

pub fn is_newer(current: &str, latest: &str) -> Result<bool> {
    let mut current = version(current)?;
    let mut latest = version(latest)?;
    if !latest.pre.is_empty() {
        return Ok(false);
    }
    // Build metadata identifies a build, not a newer release.
    current.build = semver::BuildMetadata::EMPTY;
    latest.build = semver::BuildMetadata::EMPTY;
    Ok(latest > current)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installation {
    WindowsInstalled,
    WindowsPortable,
    MacApp,
}

impl Installation {
    pub fn asset_name(self, tag: &str) -> String {
        let suffix = match self {
            Self::WindowsInstalled => "windows-x64-setup.exe",
            Self::WindowsPortable => "windows-x64-portable.zip",
            Self::MacApp => "macos-arm64-portable.zip",
        };
        format!("tiny-md-{tag}-{suffix}")
    }
}

impl Release {
    pub fn asset(&self, kind: Installation) -> Result<Asset> {
        let name = kind.asset_name(&self.version);
        self.assets
            .iter()
            .find(|a| a.name == name)
            .cloned()
            .with_context(|| format!("该版本没有匹配的更新包 {name}，请从正式发布页面安装"))
    }
}

pub fn asset_url(tag: &str, name: &str) -> String {
    format!(
        "https://github.com/{REPOSITORY}/releases/download/{}/{}",
        tag.replace('+', "%2B"),
        name.replace('+', "%2B")
    )
}

pub fn validate_digest(digest: &str) -> Result<&str> {
    let hex = digest
        .strip_prefix("sha256:")
        .context("更新包缺少 SHA-256 校验和")?;
    ensure!(
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        "SHA-256 格式无效"
    );
    Ok(hex)
}

pub fn validate_asset(tag: &str, asset: &Asset, allow_unknown_size: bool) -> Result<()> {
    version(tag)?;
    let expected = [
        Installation::WindowsInstalled.asset_name(tag),
        Installation::WindowsPortable.asset_name(tag),
        Installation::MacApp.asset_name(tag),
        format!("tiny-md-{tag}-macos-arm64.dmg"),
    ];
    ensure!(expected.contains(&asset.name), "发布附件名与平台约定不一致");
    ensure!(
        asset.url == asset_url(tag, &asset.name),
        "更新包地址不属于本仓库对应版本"
    );
    ensure!(
        (allow_unknown_size || asset.size > 0) && asset.size <= MAX_DOWNLOAD,
        "更新包大小无效"
    );
    validate_digest(&asset.digest)?;
    Ok(())
}

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    version: String,
    notes_url: String,
    assets: Vec<Asset>,
}

pub fn parse_manifest(text: &str) -> Result<Release> {
    let data: Manifest = serde_json::from_str(text).context("更新清单 JSON 无效")?;
    ensure!(data.schema_version == 1, "不支持该更新清单格式");
    ensure!(
        data.version.starts_with('v') && version(&data.version)?.pre.is_empty(),
        "稳定通道拒绝预发布或无效标签"
    );
    ensure!(
        data.notes_url == asset_url(&data.version, "release-notes.md"),
        "变更日志地址不属于对应版本"
    );
    ensure!(!data.assets.is_empty(), "更新清单没有附件");
    let mut names = std::collections::HashSet::new();
    for asset in &data.assets {
        validate_asset(&data.version, asset, false)?;
        ensure!(names.insert(&asset.name), "更新清单有重复附件");
    }
    Ok(Release {
        version: data.version,
        notes: String::new(),
        assets: data.assets,
    })
}

pub(crate) fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let host = attempt.url().host_str().unwrap_or_default();
        if attempt.previous().len() >= 6
            || attempt.url().scheme() != "https"
            || ![
                "github.com",
                "api.github.com",
                "release-assets.githubusercontent.com",
                "objects.githubusercontent.com",
                "github-releases.githubusercontent.com",
            ]
            .contains(&host)
        {
            attempt.error("更新请求重定向到非受信 HTTPS 地址")
        } else {
            attempt.follow()
        }
    })
}

pub(crate) fn user_notes(text: &str) -> String {
    let mut skipping = false;
    text.lines()
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
        .join("\n")
        .trim()
        .into()
}
