use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme, Disableable, Root, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    text::{TextView, TextViewStyle},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tiny_md_updater::{
    CURRENT_VERSION, Cancellation, CheckMode, Installation, Preferences, PreparedUpdate, Release,
};

const CURRENT_NOTES: &str = include_str!(concat!(env!("OUT_DIR"), "/release-notes.md"));

#[derive(Debug, PartialEq, Eq)]
enum DiscoveryAction {
    Available,
    NotifyAvailable,
    DownloadInBackground,
}

fn discovery_action(
    auto_download: bool,
    manual: bool,
    can_download: bool,
    development: bool,
) -> DiscoveryAction {
    if development {
        DiscoveryAction::Available
    } else if auto_download {
        if can_download {
            DiscoveryAction::DownloadInBackground
        } else {
            DiscoveryAction::Available
        }
    } else if manual {
        DiscoveryAction::Available
    } else {
        DiscoveryAction::NotifyAvailable
    }
}

fn notify_download_complete(
    succeeded: bool,
    automatic: bool,
    enabled: bool,
    cancelled: bool,
) -> bool {
    succeeded && automatic && enabled && !cancelled
}

enum Phase {
    Idle,
    Checking,
    Current(Release),
    Available(Release),
    Downloading {
        release: Release,
        cancel: Cancellation,
        progress: Arc<AtomicU64>,
        total: Arc<AtomicU64>,
        automatic: bool,
    },
    Ready(Release, Arc<PreparedUpdate>),
    Preparing(Release, Arc<PreparedUpdate>),
}
impl Phase {
    fn blocks_check(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Ready(..) | Self::Preparing(..)
        )
    }
    fn release(&self) -> Option<&Release> {
        match self {
            Self::Current(r)
            | Self::Available(r)
            | Self::Ready(r, _)
            | Self::Preparing(r, _)
            | Self::Downloading { release: r, .. } => Some(r),
            _ => None,
        }
    }
}

pub(super) struct UpdateCenter {
    phase: Phase,
    prefs: Preferences,
    prefs_path: Option<std::path::PathBuf>,
    kind: Option<Installation>,
    notice: String,
    current_notes: bool,
    request: u64,
    _progress: Option<Task<()>>,
    _timer: Option<Task<()>>,
}
struct Updates {
    center: Entity<UpdateCenter>,
    window: Option<AnyWindowHandle>,
}
impl Global for Updates {}

pub(super) fn preferences(cx: &App) -> Preferences {
    cx.global::<Updates>().center.read(cx).prefs.clone()
}

pub(super) fn set_preferences(
    mode: CheckMode,
    interval_hours: u32,
    cx: &mut App,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        (1..=8760).contains(&interval_hours),
        "检查间隔必须为 1–8760 小时"
    );
    cx.global::<Updates>()
        .center
        .clone()
        .update(cx, |this, cx| {
            let mut candidate = this.prefs.clone();
            candidate.mode = mode;
            candidate.interval_hours = interval_hours;
            let path = this
                .prefs_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("无法找到配置目录"))?;
            candidate.save(path)?;
            this.prefs = candidate;
            cx.notify();
            Ok(())
        })
}

pub(super) fn set_auto_download(enabled: bool, cx: &mut App) -> anyhow::Result<()> {
    cx.global::<Updates>()
        .center
        .clone()
        .update(cx, |this, cx| {
            let mut candidate = this.prefs.clone();
            candidate.auto_download = enabled;
            candidate.save(
                this.prefs_path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("无法找到配置目录"))?,
            )?;
            this.prefs = candidate;
            if !enabled
                && let Phase::Downloading {
                    automatic: true,
                    cancel,
                    ..
                } = &this.phase
            {
                cancel.cancel();
            }
            cx.notify();
            Ok(())
        })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn init(cx: &mut App) {
    let path = tiny_md_updater::preferences_path().ok();
    init_at(path, cx);
}

pub(super) fn init_at(path: Option<std::path::PathBuf>, cx: &mut App) {
    let loaded = path.as_ref().map(|p| Preferences::load(p));
    let (prefs, notice) = match loaded {
        Some(Ok(p)) => (p, String::new()),
        Some(Err(e)) => (
            Preferences {
                mode: CheckMode::Disabled,
                ..Default::default()
            },
            format!("无法读取更新设置，自动检查已关闭：{e}"),
        ),
        None => (
            Preferences {
                mode: CheckMode::Disabled,
                ..Default::default()
            },
            "无法找到配置目录，自动检查已关闭".into(),
        ),
    };
    let center = cx.new(|_| UpdateCenter {
        phase: Phase::Idle,
        prefs,
        prefs_path: path,
        kind: tiny_md_updater::install::installation().ok(),
        notice,
        current_notes: false,
        request: 0,
        _progress: None,
        _timer: None,
    });
    cx.set_global(Updates {
        center: center.clone(),
        window: None,
    });
    center.update(cx, |this, cx| {
        this._timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let mut first = true;
            loop {
                let keep = this
                    .update(cx, |this, cx| {
                        let due = match this.prefs.mode {
                            CheckMode::Startup => first,
                            CheckMode::Interval => {
                                this.prefs.last_checked == 0 || this.prefs.delay(now()).is_zero()
                            }
                            CheckMode::Disabled => false,
                        };
                        // A source build must never update the compiler's output.
                        if due && !cfg!(debug_assertions) {
                            this.check(false, cx);
                        }
                        first = false;
                    })
                    .is_ok();
                if !keep {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
            }
        }));
    });
}

pub(super) fn show(cx: &mut App, check: bool, current_notes: bool) {
    let center = cx.global::<Updates>().center.clone();
    center.update(cx, |this, cx| {
        this.current_notes = current_notes;
        if check {
            this.check(true, cx);
        }
        cx.notify();
    });
    if let Some(handle) = cx.global::<Updates>().window
        && cx.windows().contains(&handle)
    {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let bounds = Bounds::centered(None, size(px(680.0), px(610.0)), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(520.0), px(400.0))),
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        },
        |window, cx| {
            window.on_window_should_close(cx, |_, _| true);
            cx.new(|cx| Root::new(center, window, cx))
        },
    ) {
        Ok(handle) => cx.global_mut::<Updates>().window = Some(handle.into()),
        Err(e) => eprintln!("无法打开更新窗口：{e}"),
    }
}

impl UpdateCenter {
    fn persist(&mut self) {
        if let Some(path) = &self.prefs_path
            && let Err(e) = self.prefs.save(path)
        {
            self.notice = format!("更新设置保存失败：{e}");
        }
    }
    fn check(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.phase.blocks_check() {
            return;
        }
        self.phase = Phase::Checking;
        self.notice.clear();
        self.current_notes = false;
        self.prefs.last_checked = now();
        self.persist();
        self.request += 1;
        let request = self.request;
        let task = cx
            .background_executor()
            .spawn(async { tiny_md_updater::check() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.request != request {
                    return;
                }
                match result.and_then(|release| {
                    tiny_md_updater::is_newer(CURRENT_VERSION, &release.version)
                        .map(|newer| (release, newer))
                }) {
                    Ok((release, true)) => {
                        let can_download =
                            this.kind.is_some_and(|kind| release.asset(kind).is_ok());
                        this.phase = Phase::Available(release);
                        match discovery_action(
                            this.prefs.auto_download,
                            manual,
                            can_download,
                            cfg!(debug_assertions),
                        ) {
                            DiscoveryAction::DownloadInBackground => this.download(true, cx),
                            DiscoveryAction::NotifyAvailable => {
                                cx.defer(|cx| show(cx, false, false))
                            }
                            DiscoveryAction::Available => {}
                        }
                    }
                    Ok((release, false)) => this.phase = Phase::Current(release),
                    Err(e) => {
                        this.phase = Phase::Idle;
                        this.notice = format!("检查更新失败：{e:#}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn download(&mut self, automatic: bool, cx: &mut Context<Self>) {
        let Phase::Available(release) = &self.phase else {
            return;
        };
        let release = release.clone();
        let Some(kind) = self.kind else {
            return;
        };
        let cancel = Cancellation::default();
        let progress = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        self.phase = Phase::Downloading {
            release: release.clone(),
            cancel: cancel.clone(),
            progress: progress.clone(),
            total: total.clone(),
            automatic,
        };
        self.notice.clear();
        self.request += 1;
        let request = self.request;
        self._progress = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        if matches!(this.phase, Phase::Downloading { .. }) {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
        let worker_release = release.clone();
        let worker_cancel = cancel.clone();
        let task = cx.background_executor().spawn(async move {
            tiny_md_updater::download(&worker_release, kind, &worker_cancel, |done, size| {
                progress.store(done, Ordering::Relaxed);
                total.store(size, Ordering::Relaxed);
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.complete_download(request, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn complete_download(
        &mut self,
        request: u64,
        result: anyhow::Result<PreparedUpdate>,
        cx: &mut Context<Self>,
    ) {
        if self.request != request {
            return;
        }
        let Phase::Downloading {
            release,
            automatic,
            cancel,
            ..
        } = &self.phase
        else {
            return;
        };
        let release = release.clone();
        let cancelled = cancel.is_cancelled();
        let notify = notify_download_complete(
            result.is_ok(),
            *automatic,
            self.prefs.auto_download,
            cancelled,
        );
        self._progress = None;
        match result {
            Ok(prepared) if !cancelled => self.phase = Phase::Ready(release, Arc::new(prepared)),
            Ok(_) => {
                self.phase = Phase::Available(release);
                self.notice = "下载已取消".into();
            }
            Err(e) => {
                self.phase = Phase::Available(release);
                self.notice = if cancelled {
                    "下载已取消".into()
                } else {
                    format!("下载失败：{e:#}")
                };
            }
        }
        if notify {
            // The preference may change before the deferred activation runs.
            cx.defer(|cx| {
                let center = cx.global::<Updates>().center.clone();
                if center.read(cx).prefs.auto_download
                    && matches!(center.read(cx).phase, Phase::Ready(..))
                {
                    show(cx, false, false);
                }
            });
        }
        cx.notify();
    }
    fn confirm_install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.phase, Phase::Ready(..)) {
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "安装更新并重启？",
            Some("将逐窗口确认未保存文档，然后安装更新并重新打开 Tiny MD。"),
            &["安装并重启", "取消"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if matches!(answer.await, Ok(0)) {
                let _ = this.update(cx, |_, cx| cx.defer(crate::begin_update));
            }
        })
        .detach();
    }
}

pub(super) fn prepare(cx: &mut App) -> bool {
    cx.global::<Updates>()
        .center
        .clone()
        .update(cx, |this, cx| {
            if let Phase::Ready(release, prepared) = &this.phase {
                this.phase = Phase::Preparing(release.clone(), prepared.clone());
                this.notice = "正在确认未保存文档……".into();
                cx.notify();
                true
            } else {
                false
            }
        })
}
pub(super) fn cancel_preparation(cx: &mut App, message: &str) {
    cx.global::<Updates>()
        .center
        .clone()
        .update(cx, |this, cx| {
            if let Phase::Preparing(release, prepared) = &this.phase {
                this.phase = Phase::Ready(release.clone(), prepared.clone());
                this.notice = message.into();
                cx.notify();
            }
        });
}
pub(super) fn install(cx: &mut App) {
    let center = cx.global::<Updates>().center.clone();
    let prepared = match &center.read(cx).phase {
        Phase::Preparing(_, prepared) => prepared.clone(),
        _ => return,
    };
    center.update(cx, |this, cx| {
        this.notice = "正在启动安装助手……".into();
        cx.notify();
    });
    let task = cx
        .background_executor()
        .spawn(async move { tiny_md_updater::install::launch(&prepared) });
    cx.spawn(async move |cx| {
        let result = task.await;
        let _ = cx.update(|cx| match result {
            Ok(()) => cx.quit(),
            Err(e) => {
                crate::abort_update(cx);
                center.update(cx, |this, cx| {
                    this.notice = format!("安装未开始：{e:#}");
                    cx.notify();
                });
            }
        });
    })
    .detach();
}

fn notes_date(notes: &str) -> Option<&str> {
    notes
        .lines()
        .find_map(|line| line.strip_prefix("> 发布日期："))
}

fn notes_body(notes: &str) -> String {
    let mut lines = notes.lines();
    let first = lines.next().unwrap_or_default();
    // The version and date now belong to the header. Strip only our own
    // generated metadata; preserve arbitrary Markdown headings and quotes.
    if !first.starts_with("# Tiny MD v") {
        return notes.to_owned();
    }
    let mut metadata = true;
    lines
        .filter(|line| {
            if metadata && line.starts_with("> 发布日期：") {
                return false;
            }
            if !line.trim().is_empty() {
                metadata = false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

impl Render for UpdateCenter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Tiny MD · 在线更新");
        let dark = cx.global::<crate::Session>().dark;
        let surface = rgb(if dark { 0x222529 } else { 0xffffff });
        let quiet = rgb(if dark { 0x1e2024 } else { 0xf6f5f4 });
        let ink = rgb(if dark { 0xe4e6e9 } else { 0x37352f });
        let muted = rgb(if dark { 0xb2b7bf } else { 0x787671 });
        let border = rgb(if dark { 0x34383f } else { 0xe5e3df });
        let status: String = match &self.phase {
            Phase::Idle => "尚未检查更新".into(),
            Phase::Checking => "正在检查更新……".into(),
            Phase::Current(_) => "已是最新版本".into(),
            Phase::Available(_) => "发现新版本".into(),
            Phase::Ready(..) => "更新已准备好".into(),
            Phase::Preparing(..) => "正在准备安装……".into(),
            Phase::Downloading {
                progress,
                total,
                cancel,
                automatic,
                ..
            } => {
                let done = progress.load(Ordering::Relaxed);
                let size = total.load(Ordering::Relaxed);
                if cancel.is_cancelled() {
                    "正在取消下载……".into()
                } else if size > 0 {
                    if done >= size {
                        "正在校验更新包……".into()
                    } else if *automatic {
                        "正在后台下载更新".into()
                    } else {
                        "正在下载更新".into()
                    }
                } else if *automatic {
                    "正在连接，准备后台下载……".into()
                } else {
                    "正在连接下载……".into()
                }
            }
        };
        let new_version = matches!(
            self.phase,
            Phase::Available(_)
                | Phase::Downloading { .. }
                | Phase::Ready(..)
                | Phase::Preparing(..)
        );
        let version = if new_version {
            self.phase
                .release()
                .map(|release| release.version.trim_start_matches('v'))
                .unwrap_or(CURRENT_VERSION)
        } else {
            CURRENT_VERSION
        };
        let header_notes = if new_version {
            self.phase
                .release()
                .map(|release| release.notes.as_str())
                .unwrap_or("")
        } else {
            CURRENT_NOTES
        };
        let subtitle = match notes_date(header_notes) {
            Some(date) if new_version => format!("当前版本 {CURRENT_VERSION} · 新版发布于 {date}"),
            Some(date) => format!("当前版本发布于 {date}"),
            None => format!("当前版本 {CURRENT_VERSION}"),
        };
        let show_current_notes = self.current_notes || !new_version;
        let raw_notes = if show_current_notes {
            CURRENT_NOTES
        } else {
            self.phase
                .release()
                .map(|release| release.notes.as_str())
                .filter(|notes| !notes.is_empty())
                .unwrap_or("检查更新后显示新版本日志；也可阅读当前版本日志。")
        };
        let notes_style = TextViewStyle {
            paragraph_gap: rems(1.0),
            heading_base_font_size: px(14.0),
            highlight_theme: cx.theme().highlight_theme.clone(),
            is_dark: dark,
            ..Default::default()
        }
        .heading_font_size(|level, _| {
            px(match level {
                1 => 20.0,
                2 => 17.0,
                3 => 15.0,
                _ => 14.0,
            })
        })
        .code_block(
            StyleRefinement::default()
                .bg(quiet)
                .text_color(ink)
                .text_size(px(12.0))
                .rounded(px(4.0))
                .p_3(),
        );
        let notes_view = TextView::markdown(
            (
                "release-notes-markdown",
                usize::from(dark) + 2 * usize::from(show_current_notes),
            ),
            notes_body(raw_notes),
            window,
            cx,
        )
        .style(notes_style)
        .selectable(true)
        .scrollable(true);
        let can_download = self.kind.is_some_and(|kind| {
            self.phase
                .release()
                .is_some_and(|release| release.asset(kind).is_ok())
        });
        let primary = match &self.phase {
            Phase::Available(_) => Button::new("download-update")
                .primary()
                .label("下载更新")
                .disabled(!can_download)
                .on_click(cx.listener(|this, _, _, cx| this.download(false, cx))),
            Phase::Downloading { cancel, .. } => {
                let cancel = cancel.clone();
                Button::new("cancel-download")
                    .label("取消下载")
                    .disabled(cancel.is_cancelled())
                    .on_click(move |_, _, cx| {
                        cancel.cancel();
                        cx.refresh_windows();
                    })
            }
            Phase::Ready(..) => Button::new("install-update")
                .primary()
                .label("安装并重启")
                .disabled(cfg!(debug_assertions))
                .on_click(cx.listener(|this, _, window, cx| this.confirm_install(window, cx))),
            Phase::Preparing(..) => Button::new("preparing-update")
                .primary()
                .label("准备安装")
                .disabled(true),
            _ => Button::new("check-update")
                .primary()
                .label(if matches!(self.phase, Phase::Checking) {
                    "检查中…"
                } else {
                    "检查更新"
                })
                .disabled(self.phase.blocks_check())
                .on_click(cx.listener(|this, _, _, cx| this.check(true, cx))),
        };
        let hint = if matches!(self.phase, Phase::Available(_)) && !can_download {
            Some("没有匹配的更新包，可从发布页面手动安装。")
        } else if matches!(self.phase, Phase::Ready(..)) && cfg!(debug_assertions) {
            Some("开发构建仅支持下载，安装请使用发行包。")
        } else if matches!(self.phase, Phase::Ready(..)) {
            Some("更新包已通过校验，安装前会确认未保存内容。")
        } else if matches!(
            self.phase,
            Phase::Downloading {
                automatic: true,
                ..
            }
        ) {
            Some("可关闭窗口继续写作，下载校验完成后会再次提示。")
        } else {
            None
        };
        let status_color = match self.phase {
            Phase::Available(_) | Phase::Ready(..) | Phase::Current(_) => {
                rgb(if dark { 0x8eac98 } else { 0x3f7154 })
            }
            Phase::Checking | Phase::Downloading { .. } | Phase::Preparing(..) => {
                rgb(if dark { 0xb4a4e5 } else { 0x6b50b6 })
            }
            Phase::Idle => muted,
        };
        let mut header = div()
            .flex_shrink_0()
            .px_8()
            .pt_6()
            .pb_5()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_size(px(22.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Tiny MD"),
                                    )
                                    .child(
                                        div()
                                            .bg(quiet)
                                            .text_color(muted)
                                            .text_size(px(13.0))
                                            .px_2()
                                            .py_1()
                                            .rounded(px(4.0))
                                            .child(format!("v{version}")),
                                    ),
                            )
                            .child(div().text_size(px(12.0)).text_color(muted).child(subtitle)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .debug_selector(|| "update-primary".into())
                            .child(primary.h(px(36.0)).rounded(px(8.0)).px_4()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(status_color)
                    .child(
                        div()
                            .size(px(6.0))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(status_color),
                    )
                    .child(status),
            );
        if let Phase::Downloading {
            progress, total, ..
        } = &self.phase
        {
            let size = total.load(Ordering::Relaxed);
            let done = progress.load(Ordering::Relaxed);
            let detail = if size > 0 {
                format!(
                    "{:.0}% · {:.1} / {:.1} MB",
                    (done as f64 * 100.0 / size as f64).min(100.0),
                    done as f64 / 1_048_576.0,
                    size as f64 / 1_048_576.0
                )
            } else {
                format!("已下载 {:.1} MB", done as f64 / 1_048_576.0)
            };
            header = header.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_size(px(12.0)).text_color(muted).child(detail))
                    .when(size > 0, |row| {
                        row.child(
                            gpui_component::progress::Progress::new()
                                .value((done as f32 * 100.0 / size as f32).min(100.0))
                                .h(px(4.0)),
                        )
                    }),
            );
        }
        if let Some(hint) = hint {
            header = header.child(div().text_size(px(12.0)).text_color(muted).child(hint));
        }
        if !self.notice.is_empty() {
            header = header.child(
                div()
                    .text_size(px(12.0))
                    .text_color(muted)
                    .child(self.notice.clone()),
            );
        }
        let log_version = if show_current_notes {
            CURRENT_VERSION
        } else {
            version
        };
        let tabs = div()
            .h(px(48.0))
            .flex_shrink_0()
            .mx_8()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("更新日志"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(muted)
                            .child(format!("v{log_version}")),
                    ),
            )
            .when(new_version, |bar| {
                bar.child(
                    div()
                        .flex()
                        .gap_1()
                        .bg(quiet)
                        .p_1()
                        .rounded(px(6.0))
                        .child(
                            div().debug_selector(|| "update-new-notes".into()).child(
                                Button::new("new-notes")
                                    .ghost()
                                    .small()
                                    .border_0()
                                    .label("新版本")
                                    .h(px(26.0))
                                    .rounded(px(4.0))
                                    .text_color(muted)
                                    .when(!show_current_notes, |button| {
                                        button
                                            .bg(surface)
                                            .text_color(ink)
                                            .font_weight(FontWeight::MEDIUM)
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.current_notes = false;
                                        cx.notify();
                                    })),
                            ),
                        )
                        .child(
                            Button::new("current-notes")
                                .ghost()
                                .small()
                                .border_0()
                                .label("当前版本")
                                .h(px(26.0))
                                .rounded(px(4.0))
                                .text_color(muted)
                                .when(show_current_notes, |button| {
                                    button
                                        .bg(surface)
                                        .text_color(ink)
                                        .font_weight(FontWeight::MEDIUM)
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.current_notes = true;
                                    cx.notify();
                                })),
                        ),
                )
            });
        let footer = div()
            .debug_selector(|| "update-footer".into())
            .h(px(52.0))
            .flex_shrink_0()
            .bg(surface)
            .border_t_1()
            .border_color(border)
            .px_8()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("updates-preferences")
                            .ghost()
                            .small()
                            .border_0()
                            .text_color(muted)
                            .icon(gpui_component::IconName::Settings)
                            .label("更新设置")
                            .on_click(|_, _, cx| {
                                crate::preferences::show(crate::preferences::Category::Updates, cx)
                            }),
                    )
                    .child(
                        Button::new("release-page")
                            .ghost()
                            .small()
                            .border_0()
                            .text_color(muted)
                            .icon(gpui_component::IconName::ExternalLink)
                            .label("发布页面")
                            .on_click(|_, _, cx| {
                                cx.open_url(&format!(
                                    "https://github.com/{}/releases/latest",
                                    tiny_md_updater::REPOSITORY
                                ))
                            }),
                    ),
            )
            .child(
                Button::new("updates-done")
                    .ghost()
                    .small()
                    .border_0()
                    .label("关闭")
                    .on_click(|_, window, _| window.remove_window()),
            );
        div()
            .id("update-center")
            .size_full()
            .flex()
            .flex_col()
            .font_family(crate::ui_font())
            .text_size(px(13.0))
            .bg(surface)
            .text_color(ink)
            .child(crate::utility_titlebar(
                "Tiny MD · 在线更新",
                "updates-close",
                dark,
            ))
            .child(header)
            .child(tabs)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .debug_selector(|| "update-notes".into())
                    .px_8()
                    .py_5()
                    .text_size(px(14.0))
                    .line_height(px(22.0))
                    .overflow_hidden()
                    .child(notes_view),
            )
            .child(footer)
    }
}

#[cfg(test)]
mod preference_tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn automatic_download_waits_for_verified_completion_before_notifying() {
        assert_eq!(
            discovery_action(true, false, true, false),
            DiscoveryAction::DownloadInBackground
        );
        assert_eq!(
            discovery_action(true, true, true, false),
            DiscoveryAction::DownloadInBackground
        );
        assert_eq!(
            discovery_action(false, false, true, false),
            DiscoveryAction::NotifyAvailable
        );
        assert_eq!(
            discovery_action(false, true, true, false),
            DiscoveryAction::Available
        );
        assert_eq!(
            discovery_action(true, false, false, false),
            DiscoveryAction::Available
        );
        assert_eq!(
            discovery_action(true, true, true, true),
            DiscoveryAction::Available
        );
        assert!(notify_download_complete(true, true, true, false));
        for args in [
            (false, true, true, false),
            (true, true, true, true),
            (true, true, false, false),
            (true, false, true, false),
        ] {
            assert!(!notify_download_complete(args.0, args.1, args.2, args.3));
        }
    }

    #[gpui::test]
    fn disabling_background_download_cancels_only_automatic_tasks_and_saves_atomically(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        cx.update(|cx| {
            init_at(Some(path.clone()), cx);
            set_auto_download(true, cx).unwrap();
            set_preferences(CheckMode::Interval, 48, cx).unwrap();
            assert!(preferences(cx).auto_download);
            let center = cx.global::<Updates>().center.clone();
            for automatic in [false, true] {
                let cancel = Cancellation::default();
                center.update(cx, |center, cx| {
                    center.phase = Phase::Downloading {
                        release: Release {
                            version: "v0.3.0".into(),
                            notes: String::new(),
                            assets: vec![],
                        },
                        cancel: cancel.clone(),
                        automatic,
                        progress: Arc::new(AtomicU64::new(0)),
                        total: Arc::new(AtomicU64::new(0)),
                    };
                    let request = center.request;
                    center.check(false, cx);
                    assert_eq!(
                        center.request, request,
                        "no duplicate scheduled check during download"
                    );
                });
                set_auto_download(false, cx).unwrap();
                assert_eq!(cancel.is_cancelled(), automatic);
                set_auto_download(true, cx).unwrap();
            }
            let blocked = root.path().join("blocked");
            std::fs::write(&blocked, b"not a directory").unwrap();
            center.update(cx, |center, _| {
                center.prefs_path = Some(blocked.join("settings.json"))
            });
            assert!(set_auto_download(false, cx).is_err());
            assert!(preferences(cx).auto_download);
        });
        let saved = Preferences::load(&path).unwrap();
        assert!(saved.auto_download);
        assert_eq!(saved.interval_hours, 48);
    }

    #[gpui::test]
    fn failed_cancelled_and_stale_background_results_do_not_open_a_window(cx: &mut TestAppContext) {
        cx.update(|cx| {
            init_at(None, cx);
            let center = cx.global::<Updates>().center.clone();
            for cancelled in [false, true] {
                center.update(cx, |center, cx| {
                    let cancel = Cancellation::default();
                    if cancelled {
                        cancel.cancel();
                    }
                    center.prefs.auto_download = true;
                    center.request = 2;
                    center.phase = Phase::Downloading {
                        release: Release {
                            version: "v0.3.0".into(),
                            notes: String::new(),
                            assets: vec![],
                        },
                        cancel,
                        automatic: true,
                        progress: Arc::new(AtomicU64::new(0)),
                        total: Arc::new(AtomicU64::new(0)),
                    };
                    center.complete_download(1, Err(anyhow::anyhow!("stale error")), cx);
                    assert!(matches!(center.phase, Phase::Downloading { .. }));
                    center.complete_download(2, Err(anyhow::anyhow!("hash mismatch")), cx);
                    assert!(matches!(center.phase, Phase::Available(_)));
                    assert!(center.notice.contains(if cancelled {
                        "取消"
                    } else {
                        "hash mismatch"
                    }));
                });
            }
        });
        cx.run_until_parked();
        cx.read(|cx| {
            assert!(cx.global::<Updates>().window.is_none());
            assert!(cx.windows().is_empty());
        });
    }

    #[test]
    fn log_body_removes_only_leading_version_metadata_and_keeps_markdown_quotes() {
        let notes = "# Tiny MD v9.9.9\n\n> 发布日期：2026-10-09\n\n## 更新\n\n> 发布日期：这段引用应保留\n\n**内容**";
        assert_eq!(
            notes_body(notes),
            "## 更新\n\n> 发布日期：这段引用应保留\n\n**内容**"
        );
        assert_eq!(
            notes_body("# 自定义标题\n\n> 发布日期：保留"),
            "# 自定义标题\n\n> 发布日期：保留"
        );
    }

    #[gpui::test]
    fn update_settings_preserve_check_history_and_failure_keeps_last_effective_values(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("update-settings.json");
        Preferences {
            mode: CheckMode::Startup,
            interval_hours: 24,
            last_checked: 123,
            ..Default::default()
        }
        .save(&path)
        .unwrap();
        cx.update(|cx| {
            init_at(Some(path.clone()), cx);
            set_preferences(CheckMode::Interval, 48, cx).unwrap();
            assert_eq!(preferences(cx).mode, CheckMode::Interval);
            assert_eq!(preferences(cx).last_checked, 123);
            assert!(set_preferences(CheckMode::Disabled, 0, cx).is_err());
            assert_eq!(preferences(cx).mode, CheckMode::Interval);
            let blocked = root.path().join("blocked");
            std::fs::write(&blocked, b"not a directory").unwrap();
            cx.global::<Updates>()
                .center
                .clone()
                .update(cx, |center, _| {
                    center.prefs_path = Some(blocked.join("settings.json"))
                });
            assert!(set_preferences(CheckMode::Disabled, 48, cx).is_err());
            assert_eq!(preferences(cx).mode, CheckMode::Interval);
        });
        let saved = Preferences::load(&path).unwrap();
        assert_eq!(saved.interval_hours, 48);
        assert_eq!(saved.last_checked, 123);
    }

    #[gpui::test]
    fn update_actions_and_logs_fit_the_redesigned_minimum_window(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::guise_theme(false).init(cx);
            cx.set_global(crate::Session::default());
        });
        let release = Release {
            version: "v0.3.0".into(),
            notes: "# Tiny MD v0.3.0\n\n> 发布日期：2026-10-09\n\n".to_owned()
                + &"## 更新内容\n\n- **可读日志**\n- 保留已有笔记\n\n".repeat(30),
            assets: vec![],
        };
        for dark in [false, true] {
            for phase in 0..5 {
                // Each window has its own debug bounds cache; removed elements
                // from a prior phase must not count as visible in this phase.
                let (view, cx) = cx.add_window_view(|_, _| UpdateCenter {
                    phase: Phase::Idle,
                    prefs: Preferences::default(),
                    prefs_path: None,
                    kind: Some(Installation::WindowsPortable),
                    notice: String::new(),
                    current_notes: false,
                    request: 0,
                    _progress: None,
                    _timer: None,
                });
                cx.update(|window, cx| {
                    crate::apply_theme(dark, window, cx);
                    view.update(cx, |view, _| {
                        view.phase = match phase {
                            0 => Phase::Idle,
                            1 => Phase::Checking,
                            2 => Phase::Current(Release {
                                version: format!("v{CURRENT_VERSION}"),
                                notes: CURRENT_NOTES.into(),
                                assets: vec![],
                            }),
                            3 => Phase::Available(release.clone()),
                            _ => Phase::Downloading {
                                release: release.clone(),
                                cancel: Cancellation::default(),
                                progress: Arc::new(AtomicU64::new(100)),
                                total: Arc::new(AtomicU64::new(200)),
                                automatic: true,
                            },
                        };
                    });
                });
                cx.simulate_resize(size(px(520.0), px(400.0)));
                cx.run_until_parked();
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear();
                });
                let action = cx
                    .debug_bounds("update-primary")
                    .expect("update header action");
                assert!(action.origin.y >= px(0.0) && action.bottom() <= px(400.0));
                assert!(action.origin.x >= px(0.0) && action.right() <= px(520.0));
                assert!(
                    action.bottom() <= px(180.0),
                    "primary action belongs beside the version"
                );
                let notes = cx
                    .debug_bounds("update-notes")
                    .expect("scrollable note viewport");
                assert!(notes.size.height > px(50.0));
                let footer = cx
                    .debug_bounds("update-footer")
                    .expect("compact update footer");
                assert!(notes.bottom() <= footer.origin.y);
                assert!(footer.bottom() <= px(400.0));
                assert_eq!(cx.debug_bounds("update-new-notes").is_some(), phase >= 3);
            }
        }
    }
}
