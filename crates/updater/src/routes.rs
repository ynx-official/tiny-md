//! Network routes are transport choices, never authorities for release identity.
use crate::{
    CURRENT_VERSION, Preferences, REPOSITORY, Release, asset_url, is_newer, parse_manifest, signing,
};
use anyhow::{Context, Result, ensure};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    time::{Duration, Instant},
};

const LIMIT: u64 = 2 * 1024 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_PARALLEL_PROBES: usize = 4;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    #[default]
    Auto,
    Github,
    GhProxy,
    GhFast,
    Custom,
}
impl RouteMode {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Github,
        Self::GhProxy,
        Self::GhFast,
        Self::Custom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动择优",
            Self::Github => "仅 GitHub",
            Self::GhProxy => "gh-proxy.com",
            Self::GhFast => "ghfast.top",
            Self::Custom => "自定义线路",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub mode: RouteMode,
    pub prefix: String,
}
impl Route {
    pub fn github() -> Self {
        Self {
            mode: RouteMode::Github,
            prefix: String::new(),
        }
    }
    pub fn label(&self) -> String {
        match self.mode {
            RouteMode::Github => "GitHub 直连".into(),
            RouteMode::Custom => self
                .prefix
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .into(),
            mode => mode.label().into(),
        }
    }
    pub fn url(&self, canonical: &str) -> Result<String> {
        ensure!(
            canonical.starts_with(&format!("https://github.com/{REPOSITORY}/releases/")),
            "线路只允许访问本项目发布文件"
        );
        ensure!(
            !canonical.contains(['?', '#', '\\']) && !canonical.contains("/../"),
            "发布地址无效"
        );
        if self.mode == RouteMode::Github {
            return Ok(canonical.into());
        }
        let prefix = normalize_custom(&self.prefix)?;
        Ok(format!("{prefix}{canonical}"))
    }
    pub(crate) fn redirect_policy(&self) -> reqwest::redirect::Policy {
        let origin = Url::parse(&self.prefix).ok().map(|u| u.origin());
        reqwest::redirect::Policy::custom(move |attempt| {
            let url = attempt.url();
            let official = url.port_or_known_default() == Some(443)
                && [
                    "github.com",
                    "api.github.com",
                    "release-assets.githubusercontent.com",
                    "objects.githubusercontent.com",
                    "github-releases.githubusercontent.com",
                ]
                .contains(&url.host_str().unwrap_or_default());
            if attempt.previous().len() >= 6
                || url.scheme() != "https"
                || !url.username().is_empty()
                || url.password().is_some()
                || !(official || origin.as_ref() == Some(&url.origin()))
            {
                attempt.error("更新请求重定向到未允许的 HTTPS 地址")
            } else {
                attempt.follow()
            }
        })
    }
}

pub fn normalize_custom(value: &str) -> Result<String> {
    let mut url = Url::parse(value.trim()).context("请输入完整的 HTTPS 代理前缀")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "线路必须为 HTTPS 地址，不能含账号、密码、查询参数或片段"
    );
    let host = url.host_str().unwrap();
    ensure!(
        host != "localhost" && !host.ends_with(".localhost") && !host.ends_with(".local"),
        "线路不能指向本机地址"
    );
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
        let local = match ip {
            std::net::IpAddr::V4(ip) => {
                ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.is_multicast()
            }
            std::net::IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || ip.is_multicast()
                    || ip.to_ipv4_mapped().is_some()
            }
        };
        ensure!(!local, "线路不能指向本机或局域网地址");
    }
    let path = url.path().to_owned();
    ensure!(
        !path.contains("https:") && !path.contains("http:") && !path.contains('%'),
        "请输入代理前缀，不要包含 GitHub 文件地址"
    );
    if !path.ends_with('/') {
        url.set_path(&format!("{path}/"));
    }
    Ok(url.into())
}

pub fn configured_routes(prefs: &Preferences) -> Result<Vec<Route>> {
    let mut prefs = prefs.clone();
    prefs.normalize_routes()?;
    let mut routes = vec![
        Route::github(),
        Route {
            mode: RouteMode::GhProxy,
            prefix: "https://gh-proxy.com/".into(),
        },
        Route {
            mode: RouteMode::GhFast,
            prefix: "https://ghfast.top/".into(),
        },
    ];
    for prefix in &prefs.custom_routes {
        // Legacy settings could contain a built-in proxy as a custom prefix.
        // Avoid duplicate requests in auto mode, while preserving fixed selection.
        if prefs.route_mode == RouteMode::Custom || !routes.iter().any(|r| &r.prefix == prefix) {
            routes.push(Route {
                mode: RouteMode::Custom,
                prefix: prefix.clone(),
            });
        }
    }
    if prefs.route_mode != RouteMode::Auto {
        routes.retain(|r| {
            r.mode == prefs.route_mode
                && (r.mode != RouteMode::Custom || r.prefix == prefs.custom_route)
        });
    }
    ensure!(!routes.is_empty(), "请先填写并保存自定义线路");
    Ok(routes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeState {
    Available,
    Timeout,
    Failed,
    Invalid,
    Unverified,
    Stale,
}
impl ProbeState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Available => "可用",
            Self::Timeout => "超时",
            Self::Failed => "不可用",
            Self::Invalid => "校验失败",
            Self::Unverified => "待核验",
            Self::Stale => "版本落后",
        }
    }
}
#[derive(Clone, Debug)]
pub struct ProbeResult {
    pub route: Route,
    pub elapsed_ms: Option<u64>,
    pub state: ProbeState,
    pub detail: String,
}
#[derive(Clone, Debug, Default)]
pub struct ProbeReport {
    pub entries: Vec<ProbeResult>,
    pub release: Option<Release>,
    pub routes: Vec<Route>,
    pub error: Option<String>,
    notes_digest: Option<String>,
    candidates: Vec<(Route, Release)>,
}

impl ProbeReport {
    fn corroborate(&mut self, official: Release) {
        for entry in &mut self.entries {
            let candidate = self
                .candidates
                .iter()
                .find(|(route, _)| route == &entry.route)
                .map(|(_, release)| release);
            if entry.route.mode == RouteMode::Github {
                entry.state = ProbeState::Available;
                entry.detail = "通过 GitHub 备用接口获取".into();
            } else if let Some(candidate) = candidate {
                let matches = candidate.version == official.version
                    && candidate.assets.len() == official.assets.len()
                    && official.assets.iter().all(|asset| {
                        candidate.assets.iter().any(|other| {
                            asset.name == other.name
                                && asset.url == other.url
                                && asset.digest == other.digest
                                && (asset.size == 0 || asset.size == other.size)
                        })
                    });
                if matches {
                    entry.state = ProbeState::Available;
                } else if is_newer(&candidate.version, &official.version).unwrap_or(false) {
                    entry.state = ProbeState::Stale;
                }
            }
        }
        let mut available: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.state == ProbeState::Available)
            .collect();
        available.sort_by_key(|entry| entry.elapsed_ms.unwrap_or(u64::MAX));
        self.routes = available.iter().map(|entry| entry.route.clone()).collect();
        if !self.routes.is_empty() {
            self.release = Some(official);
            self.error = None;
            self.notes_digest = None;
        }
    }
}

/// Only this boundary is substituted in tests; routing, validation and selection
/// use the production code. Each route shares one deadline across both requests.
pub trait ProbeTransport: Sync {
    fn get(&self, route: &Route, canonical: &str, remaining: Duration) -> Result<crate::HttpText>;
}
struct Http;
impl ProbeTransport for Http {
    fn get(&self, route: &Route, canonical: &str, remaining: Duration) -> Result<crate::HttpText> {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .redirect(route.redirect_policy())
            .referer(false)
            .user_agent(format!("tiny-md/{CURRENT_VERSION}"))
            .connect_timeout(Duration::from_secs(2))
            .timeout(remaining)
            .build()?;
        let response = client.get(route.url(canonical)?).send()?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let mut body = Vec::new();
        response.take(LIMIT + 1).read_to_end(&mut body)?;
        ensure!(body.len() as u64 <= LIMIT, "更新元数据超过大小限制");
        Ok(crate::HttpText {
            status,
            final_url,
            body: String::from_utf8(body).context("更新响应不是 UTF-8")?,
        })
    }
}

struct Sample {
    result: ProbeResult,
    release: Option<Release>,
    trusted: bool,
    notes_digest: Option<String>,
}
fn probe_one(route: Route, transport: &impl ProbeTransport, key: &str) -> Sample {
    let started = Instant::now();
    let mut sample = Sample {
        result: ProbeResult {
            route: route.clone(),
            elapsed_ms: None,
            state: ProbeState::Failed,
            detail: String::new(),
        },
        release: None,
        trusted: false,
        notes_digest: None,
    };
    let result = (|| -> Result<()> {
        let response = transport.get(&route, crate::source::MANIFEST_URL, PROBE_TIMEOUT)?;
        ensure!(response.status == 200, "HTTP {}", response.status);
        sample.result.state = ProbeState::Invalid;
        let release = parse_manifest(&response.body)?;
        let value: serde_json::Value = serde_json::from_str(&response.body)?;
        sample.notes_digest = value
            .get("notes_digest")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        if let Some(digest) = &sample.notes_digest {
            crate::validate_digest(digest)?;
        }
        sample.result.elapsed_ms = Some(started.elapsed().as_millis() as u64);
        sample.trusted = route.mode == RouteMode::Github;
        if !sample.trusted {
            let remaining = PROBE_TIMEOUT.saturating_sub(started.elapsed());
            // Unsigned historical releases can only be corroborated with GitHub.
            // A present but invalid signature is never treated as missing.
            if !remaining.is_zero()
                && let Ok(signature) = transport.get(
                    &route,
                    &asset_url(&release.version, "update-manifest.json.sig"),
                    remaining,
                )
                && signature.status == 200
            {
                signing::verify(response.body.as_bytes(), &signature.body, key)?;
                sample.trusted = true;
            }
        }
        sample.result.state = if sample.trusted {
            ProbeState::Available
        } else {
            ProbeState::Unverified
        };
        sample.release = Some(release);
        Ok(())
    })();
    if let Err(error) = result {
        if error
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_timeout)
        {
            sample.result.state = ProbeState::Timeout;
        }
        // reqwest errors can contain CDN tokens or custom URLs: retain the cause,
        // not a raw request URL in UI or logs.
        sample.result.detail = if error.is::<reqwest::Error>() {
            "网络连接失败或超时".into()
        } else {
            error.to_string()
        };
    }
    sample
}

fn same_release(a: &Release, b: &Release) -> bool {
    a.version == b.version
        && a.assets.len() == b.assets.len()
        && a.assets.iter().all(|asset| b.assets.contains(asset))
}

pub fn probe_routes(prefs: &Preferences) -> Result<ProbeReport> {
    probe_with(prefs, &Http, signing::PUBLIC_KEY)
}
pub fn probe_with(
    prefs: &Preferences,
    transport: &impl ProbeTransport,
    public_key: &str,
) -> Result<ProbeReport> {
    let configured = configured_routes(prefs)?;
    let mut all = configured.clone();
    // Fixed proxy mode still needs the official trust bootstrap for legacy,
    // unsigned releases. This extra request is never used as a download route.
    if !all.iter().any(|r| r.mode == RouteMode::Github) {
        all.push(Route::github());
    }
    // A user-maintained list must not create an unbounded number of network threads.
    // Each route's deadline starts when a worker actually picks it up.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut indexed = std::thread::scope(|scope| {
        let tasks: Vec<_> = (0..all.len().min(MAX_PARALLEL_PROBES))
            .map(|_| {
                scope.spawn(|| {
                    let mut results = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(route) = all.get(index) else {
                            break;
                        };
                        results.push((index, probe_one(route.clone(), transport, public_key)));
                    }
                    results
                })
            })
            .collect();
        let batches = tasks
            .into_iter()
            .map(|task| task.join().map_err(|_| anyhow::anyhow!("线路检测任务异常")))
            .collect::<Result<Vec<_>>>()?;
        Ok::<_, anyhow::Error>(batches.into_iter().flatten().collect::<Vec<_>>())
    })?;
    indexed.sort_by_key(|(index, _)| *index);
    let mut samples: Vec<_> = indexed.into_iter().map(|(_, sample)| sample).collect();
    let trusted: Vec<_> = samples
        .iter()
        .filter(|s| s.trusted)
        .filter_map(|s| s.release.clone())
        .collect();
    // Multiple authenticated descriptions of the same version must agree.
    for (index, a) in trusted.iter().enumerate() {
        ensure!(
            trusted[index + 1..]
                .iter()
                .all(|b| a.version != b.version || same_release(a, b)),
            "可信更新来源对同一版本的文件信息不一致"
        );
    }
    for sample in &mut samples {
        if let Some(release) = &sample.release
            && !sample.trusted
            && trusted.iter().any(|r| same_release(r, release))
        {
            sample.trusted = true;
            sample.result.state = ProbeState::Available;
            // Matching assets do not authenticate arbitrary notes metadata.
            sample.notes_digest = None;
        }
    }
    let newest = trusted.iter().reduce(|a, b| {
        if is_newer(&a.version, &b.version).unwrap_or(false) {
            b
        } else {
            a
        }
    });
    for sample in &mut samples {
        if sample.trusted
            && let (Some(latest), Some(release)) = (newest, &sample.release)
            && is_newer(&release.version, &latest.version)?
        {
            sample.result.state = ProbeState::Stale;
            sample.result.detail = format!(
                "此线路返回 {}，本轮已发现 {}",
                release.version, latest.version
            );
        }
    }
    samples.retain(|s| configured.contains(&s.result.route));
    let mut usable: Vec<_> = samples
        .iter()
        .filter(|s| s.result.state == ProbeState::Available)
        .collect();
    usable.sort_by_key(|s| s.result.elapsed_ms.unwrap_or(u64::MAX));
    let best = usable.first();
    Ok(ProbeReport {
        release: best.and_then(|s| s.release.clone()),
        routes: usable.iter().map(|s| s.result.route.clone()).collect(),
        notes_digest: best.and_then(|s| s.notes_digest.clone()),
        error: best
            .is_none()
            .then(|| "没有通过校验的可用线路；旧版无签名清单需要 GitHub 直连核验".into()),
        candidates: samples
            .iter()
            .filter_map(|s| {
                s.release
                    .clone()
                    .map(|release| (s.result.route.clone(), release))
            })
            .collect(),
        entries: samples.into_iter().map(|s| s.result).collect(),
    })
}

/// Reuses the probe's verified manifest rather than fetching latest again.
pub fn check_routed(
    prefs: &Preferences,
    mut on_probe: impl FnMut(&ProbeReport),
) -> Result<ProbeReport> {
    let mut report = probe_routes(prefs)?;
    on_probe(&report);
    if report.release.is_none()
        && (matches!(prefs.route_mode, RouteMode::Auto | RouteMode::Github)
            || report
                .entries
                .iter()
                .any(|e| e.state == ProbeState::Unverified))
        && !report
            .entries
            .iter()
            .any(|e| e.route.mode == RouteMode::Github && e.state == ProbeState::Invalid)
    {
        // Preserve the existing official manifest -> API -> page fallback.
        let release = crate::check()?;
        report.corroborate(release);
        on_probe(&report);
    }
    let release = report.release.as_mut().context(
        report
            .error
            .clone()
            .unwrap_or_else(|| "没有可用更新线路".into()),
    )?;
    if release.notes.is_empty() {
        let route = if report.notes_digest.is_some() {
            report.routes.first().cloned().unwrap_or_else(Route::github)
        } else {
            Route::github()
        };
        if let Ok(notes) = Http.get(
            &route,
            &asset_url(&release.version, "release-notes.md"),
            Duration::from_secs(6),
        ) && notes.status == 200
        {
            let valid = report.notes_digest.as_ref().is_none_or(|digest| {
                use sha2::Digest;
                format!("sha256:{:x}", sha2::Sha256::digest(notes.body.as_bytes())) == *digest
            });
            if valid {
                release.notes = crate::user_notes(&notes.body);
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
