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
    },
    Ready(Release, Arc<PreparedUpdate>),
    Preparing(Release, Arc<PreparedUpdate>),
}
impl Phase {
    fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Preparing(..)
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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn init(cx: &mut App) {
    let path = tiny_md_updater::preferences_path().ok();
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
        if self.phase.busy() {
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
                        this.phase = Phase::Available(release);
                        if !manual {
                            cx.defer(|cx| show(cx, false, false));
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
    fn download(&mut self, cx: &mut Context<Self>) {
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
                if this.request != request {
                    return;
                }
                this._progress = None;
                match result {
                    Ok(prepared) => this.phase = Phase::Ready(release, Arc::new(prepared)),
                    Err(e) => {
                        this.phase = Phase::Available(release);
                        this.notice = if cancel.is_cancelled() {
                            "下载已取消".into()
                        } else {
                            format!("下载失败：{e:#}")
                        };
                    }
                }
                cx.notify();
            });
        })
        .detach();
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

impl Render for UpdateCenter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Tiny MD · 在线更新");
        let dark = cx.global::<crate::Session>().dark;
        let surface = rgb(if dark { 0x222529 } else { 0xffffff });
        let ink = rgb(if dark { 0xe4e6e9 } else { 0x37352f });
        let muted = rgb(if dark { 0xb2b7bf } else { 0x787671 });
        let border = rgb(if dark { 0x34383f } else { 0xe5e3df });
        let status = match &self.phase {
            Phase::Idle => "尚未检查更新".into(),
            Phase::Checking => "正在检查更新……".into(),
            Phase::Current(_) => "没有较新的稳定版本".into(),
            Phase::Available(r) => format!("新版本 {} 可用", r.version),
            Phase::Downloading {
                progress,
                total,
                cancel,
                ..
            } => {
                let (done, size) = (
                    progress.load(Ordering::Relaxed),
                    total.load(Ordering::Relaxed),
                );
                if cancel.is_cancelled() {
                    "正在取消下载……".into()
                } else if size > 0 {
                    format!(
                        "正在下载：{:.1}% · {:.1} / {:.1} MB",
                        done as f64 * 100.0 / size as f64,
                        done as f64 / 1_048_576.0,
                        size as f64 / 1_048_576.0
                    )
                } else {
                    format!("正在下载：{:.1} MB", done as f64 / 1_048_576.0)
                }
            }
            Phase::Ready(..) => "下载已校验，可以安装".into(),
            Phase::Preparing(..) => "正在准备安装……".into(),
        };
        let notes = if self.current_notes {
            CURRENT_NOTES.to_owned()
        } else {
            self.phase
                .release()
                .map(|r| r.notes.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "检查更新后显示新版本日志；也可阅读当前版本日志。".into())
        };
        let notes_style = TextViewStyle {
            paragraph_gap: rems(0.65),
            heading_base_font_size: px(13.0),
            highlight_theme: cx.theme().highlight_theme.clone(),
            is_dark: dark,
            ..Default::default()
        }
        .heading_font_size(|level, _| {
            px(match level {
                1 => 20.0,
                2 => 16.0,
                3 => 14.0,
                _ => 13.0,
            })
        })
        .code_block(
            StyleRefinement::default()
                .bg(rgb(if dark { 0x2b2f35 } else { 0xf6f5f4 }))
                .text_color(ink)
                .text_size(px(12.0))
                .rounded(px(3.0))
                .p_3(),
        );
        // TextView captures the syntax theme when its keyed state is created.
        // Keep separate states so switching themes also refreshes code colors.
        let notes_view = TextView::markdown(
            ("release-notes-markdown", usize::from(dark)),
            notes,
            window,
            cx,
        )
        .style(notes_style)
        .selectable(true)
        .scrollable(true);
        let mut actions = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_wrap()
            .child(
                Button::new("check-update")
                    .small()
                    .label("检查更新")
                    .disabled(self.phase.busy())
                    .on_click(cx.listener(|this, _, _, cx| this.check(true, cx))),
            )
            .child(
                Button::new("current-notes")
                    .small()
                    .ghost()
                    .label("当前版本日志")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.current_notes = true;
                        cx.notify();
                    })),
            );
        if matches!(self.phase, Phase::Available(_)) {
            let can_download = self
                .kind
                .is_some_and(|kind| self.phase.release().is_some_and(|r| r.asset(kind).is_ok()));
            actions = actions.child(
                Button::new("download-update")
                    .small()
                    .primary()
                    .label("下载更新")
                    .disabled(!can_download)
                    .on_click(cx.listener(|this, _, _, cx| this.download(cx))),
            );
            if !can_download {
                actions = actions.child(
                    div()
                        .text_color(muted)
                        .child("此版本没有适合当前安装方式的更新包"),
                );
            }
        }
        if let Phase::Downloading { cancel, .. } = &self.phase {
            let cancel = cancel.clone();
            actions = actions.child(
                Button::new("cancel-download")
                    .small()
                    .label("取消下载")
                    .disabled(cancel.is_cancelled())
                    .on_click(move |_, _, cx| {
                        cancel.cancel();
                        cx.refresh_windows();
                    }),
            );
        }
        if matches!(self.phase, Phase::Ready(..)) {
            actions = actions.child(
                Button::new("install-update")
                    .small()
                    .primary()
                    .label("安装并重启")
                    .disabled(cfg!(debug_assertions))
                    .on_click(cx.listener(|this, _, w, cx| this.confirm_install(w, cx))),
            );
        }
        let mut modes = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_wrap()
            .child(div().text_color(muted).child("自动检查"));
        for (id, mode, label) in [
            ("updates-startup", CheckMode::Startup, "启动时"),
            ("updates-interval", CheckMode::Interval, "按间隔"),
            ("updates-disabled", CheckMode::Disabled, "关闭"),
        ] {
            modes = modes.child(
                Button::new(id)
                    .small()
                    .label(label)
                    .when(self.prefs.mode == mode, |button| button.primary())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.prefs.mode = mode;
                        this.persist();
                        cx.notify();
                    })),
            );
        }
        if self.prefs.mode == CheckMode::Interval {
            modes = modes
                .child(
                    Button::new("interval-minus")
                        .small()
                        .ghost()
                        .label("−")
                        .disabled(self.prefs.interval_hours <= 1)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.prefs.interval_hours =
                                this.prefs.interval_hours.saturating_sub(1).max(1);
                            this.persist();
                            cx.notify();
                        })),
                )
                .child(format!("{} 小时", self.prefs.interval_hours))
                .child(
                    Button::new("interval-plus")
                        .small()
                        .ghost()
                        .label("+")
                        .disabled(self.prefs.interval_hours >= 8760)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.prefs.interval_hours = (this.prefs.interval_hours + 1).min(8760);
                            this.persist();
                            cx.notify();
                        })),
                );
        }
        let titlebar = if cfg!(target_os = "windows") {
            div()
                .h(px(28.0))
                .flex()
                .items_center()
                .bg(if dark { surface } else { rgb(0xf0f3f9) })
                .child(
                    div()
                        .flex_1()
                        .px_3()
                        .window_control_area(WindowControlArea::Drag)
                        .child("Tiny MD · 在线更新"),
                )
                .child(
                    Button::new("updates-close")
                        .ghost()
                        .label("×")
                        .h_full()
                        .w(px(40.0))
                        .on_click(|_, w, _| w.remove_window()),
                )
                .into_any_element()
        } else {
            TitleBar::new().child("在线更新").into_any_element()
        };
        div()
            .id("update-center")
            .size_full()
            .flex()
            .flex_col()
            .font_family(crate::ui_font())
            .text_size(px(13.0))
            .bg(surface)
            .text_color(ink)
            .child(titlebar)
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .child(format!("Tiny MD {CURRENT_VERSION}")),
                    )
                    .when(cfg!(debug_assertions), |element| {
                        element.child(
                            div()
                                .text_color(muted)
                                .child("开发构建：可手动检查和下载，安装需使用发行包。"),
                        )
                    })
                    .child(modes)
                    .child(div().child(status))
                    .child(actions)
                    .when(!self.notice.is_empty(), |element| {
                        element.child(div().text_color(muted).child(self.notice.clone()))
                    }),
            )
            .child(
                div()
                    .id("release-notes-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .p_4()
                    .border_t_1()
                    .border_color(border)
                    .child(notes_view),
            )
            .child(
                div()
                    .p_3()
                    .border_t_1()
                    .border_color(border)
                    .flex()
                    .justify_end()
                    .child(
                        Button::new("release-page")
                            .small()
                            .ghost()
                            .label("打开发布页面")
                            .on_click(|_, _, cx| {
                                cx.open_url(
                                    "https://github.com/ynx-official/tiny-md/releases/latest",
                                )
                            }),
                    ),
            )
    }
}
