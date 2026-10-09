use anyhow::{Context as _, Result};
use gpui::{prelude::*, *};
use gpui_component::{
    Root, TitleBar,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tiny_md_updater::CheckMode;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct AppSettings {
    dark: bool,
}

impl AppSettings {
    fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("偏好设置格式错误"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().context("偏好设置缺少目录")?;
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

struct PreferencesState {
    settings: AppSettings,
    path: Option<PathBuf>,
    error: String,
    window: Option<AnyWindowHandle>,
    view: Option<WeakEntity<PreferencesView>>,
}
impl Global for PreferencesState {}

pub(super) fn init(cx: &mut App) -> bool {
    let path = tiny_md_updater::preferences_path()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("preferences.json")));
    init_at(path, cx)
}

pub(super) fn init_at(path: Option<PathBuf>, cx: &mut App) -> bool {
    let loaded = path.as_ref().map_or_else(
        || Err(anyhow::anyhow!("无法找到配置目录")),
        |path| AppSettings::load(path),
    );
    let (settings, error) = match loaded {
        Ok(settings) => (settings, String::new()),
        Err(error) => (
            AppSettings::default(),
            format!("无法读取偏好设置：{error:#}"),
        ),
    };
    let dark = settings.dark;
    cx.set_global(PreferencesState {
        settings,
        path,
        error,
        window: None,
        view: None,
    });
    dark
}

pub(super) fn save_theme(dark: bool, cx: &mut App) -> Result<()> {
    // Isolated document tests do not initialize the application settings store.
    if !cx.has_global::<PreferencesState>() {
        return Ok(());
    }
    let state = cx.global_mut::<PreferencesState>();
    let candidate = AppSettings { dark };
    candidate.save(state.path.as_ref().context("无法找到配置目录")?)?;
    state.settings = candidate;
    state.error.clear();
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    Appearance,
    Updates,
}

pub(super) fn show(category: Category, cx: &mut App) {
    if !cx.has_global::<PreferencesState>() {
        init(cx);
    }
    let state = cx.global::<PreferencesState>();
    if let Some(handle) = state.window
        && cx.windows().contains(&handle)
    {
        let view = state.view.clone();
        let _ = handle.update(cx, |_, window, cx| {
            if let Some(view) = view {
                let _ = view.update(cx, |view, cx| {
                    view.category = category;
                    cx.notify();
                });
            }
            window.activate_window();
        });
        return;
    }
    let bounds = Bounds::centered(None, size(px(780.0), px(540.0)), cx);
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(640.0), px(430.0))),
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| PreferencesView::new(category, window, cx));
            cx.global_mut::<PreferencesState>().view = Some(view.downgrade());
            window.focus(&view.read(cx).focus);
            cx.new(|cx| Root::new(view, window, cx))
        },
    );
    match result {
        Ok(handle) => cx.global_mut::<PreferencesState>().window = Some(handle.into()),
        Err(error) => eprintln!("无法打开偏好设置：{error}"),
    }
}

fn mode_label(mode: CheckMode) -> &'static str {
    match mode {
        CheckMode::Startup => "启动时",
        CheckMode::Interval => "按间隔",
        CheckMode::Disabled => "关闭",
    }
}

fn parse_interval(value: &str) -> Result<u32> {
    let hours: u32 = value
        .trim()
        .parse()
        .context("请输入 1–8760 之间的整数小时")?;
    anyhow::ensure!((1..=8760).contains(&hours), "检查间隔必须为 1–8760 小时");
    Ok(hours)
}

struct PreferencesView {
    category: Category,
    focus: FocusHandle,
    theme: Entity<SelectState<Vec<&'static str>>>,
    update_mode: Entity<SelectState<Vec<&'static str>>>,
    interval: Entity<InputState>,
    notice: String,
    _subscriptions: Vec<Subscription>,
}
impl PreferencesView {
    fn new(category: Category, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prefs = crate::updates::preferences(cx);
        let dark = cx.global::<crate::Session>().dark;
        let theme = cx.new(|cx| {
            SelectState::new(
                vec!["浅色", "深色"],
                Some(gpui_component::IndexPath::default().row(usize::from(dark))),
                window,
                cx,
            )
        });
        let mode_index = match prefs.mode {
            CheckMode::Startup => 0,
            CheckMode::Interval => 1,
            CheckMode::Disabled => 2,
        };
        let update_mode = cx.new(|cx| {
            SelectState::new(
                vec!["启动时", "按间隔", "关闭"],
                Some(gpui_component::IndexPath::default().row(mode_index)),
                window,
                cx,
            )
        });
        let interval = cx.new(|cx| InputState::new(window, cx).placeholder("24"));
        interval.update(cx, |input, cx| {
            input.set_value(prefs.interval_hours.to_string(), window, cx)
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &theme,
                window,
                |this, _, event: &SelectEvent<Vec<&'static str>>, window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        match save_theme(*value == "深色", cx) {
                            Ok(()) => {
                                this.notice.clear();
                                crate::apply_theme(*value == "深色", window, cx);
                            }
                            Err(error) => this.notice = format!("主题设置保存失败：{error:#}"),
                        }
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &update_mode,
                window,
                |this, _, event: &SelectEvent<Vec<&'static str>>, _, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        let mode = match *value {
                            "启动时" => CheckMode::Startup,
                            "按间隔" => CheckMode::Interval,
                            _ => CheckMode::Disabled,
                        };
                        let hours = crate::updates::preferences(cx).interval_hours;
                        this.notice = crate::updates::set_preferences(mode, hours, cx)
                            .err()
                            .map(|error| format!("更新设置保存失败：{error:#}"))
                            .unwrap_or_default();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &interval,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.save_interval(window, cx);
                    }
                },
            ),
        ];
        Self {
            category,
            focus: cx.focus_handle(),
            theme,
            update_mode,
            interval,
            notice: String::new(),
            _subscriptions: subscriptions,
        }
    }

    fn save_interval(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let prefs = crate::updates::preferences(cx);
        let result = parse_interval(&self.interval.read(cx).value())
            .and_then(|hours| crate::updates::set_preferences(prefs.mode, hours, cx));
        self.notice = match result {
            Ok(()) => "检查间隔已保存".into(),
            Err(error) => format!("检查间隔未保存：{error:#}"),
        };
        cx.notify();
    }
}
impl Focusable for PreferencesView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PreferencesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title("Tiny MD · 偏好设置");
        let dark = cx.global::<crate::Session>().dark;
        let surface = rgb(if dark { 0x222529 } else { 0xffffff });
        let quiet = rgb(if dark { 0x1e2024 } else { 0xf6f5f4 });
        let ink = rgb(if dark { 0xe4e6e9 } else { 0x37352f });
        let muted = rgb(if dark { 0xb2b7bf } else { 0x787671 });
        let border = rgb(if dark { 0x34383f } else { 0xe5e3df });
        let theme_label = if dark { "深色" } else { "浅色" };
        if self.theme.read(cx).selected_value() != Some(&theme_label) {
            self.theme.update(cx, |select, cx| {
                select.set_selected_value(&theme_label, window, cx)
            });
        }
        let prefs = crate::updates::preferences(cx);
        let mode = mode_label(prefs.mode);
        if self.update_mode.read(cx).selected_value() != Some(&mode) {
            self.update_mode.update(cx, |select, cx| {
                select.set_selected_value(&mode, window, cx)
            });
        }
        let titlebar = crate::utility_titlebar("Tiny MD · 偏好设置", "preferences-close", dark);
        let mut sidebar = div()
            .w(px(156.0))
            .flex_shrink_0()
            .h_full()
            .bg(quiet)
            .border_r_1()
            .border_color(border)
            .p_4()
            .flex()
            .flex_col()
            .gap_2();
        for (id, label, category) in [
            ("prefs-appearance", "外观", Category::Appearance),
            ("prefs-updates", "更新", Category::Updates),
        ] {
            sidebar = sidebar.child(
                div().debug_selector(move || id.into()).child(
                    Button::new(id)
                        .label(label)
                        .ghost()
                        .w_full()
                        .justify_start()
                        .h(px(36.0))
                        .when(self.category == category, |button| {
                            button
                                .bg(if dark { rgb(0x34383f) } else { rgb(0xe9e7e3) })
                                .font_weight(FontWeight::MEDIUM)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.category = category;
                            cx.notify();
                        })),
                ),
            );
        }
        let heading = match self.category {
            Category::Appearance => "外观",
            Category::Updates => "更新",
        };
        let row = |label: &'static str, control: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .w(px(128.0))
                        .flex_shrink_0()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
                .child(control)
        };
        let mut content = div()
            .id("preferences-content")
            .debug_selector(|| "preferences-content".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                div()
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(heading),
            );
        content = match self.category {
            Category::Appearance => content
                .child(row(
                    "界面主题",
                    Select::new(&self.theme).w(px(240.0)).into_any_element(),
                ))
                .child(
                    div()
                        .text_color(muted)
                        .child("应用于所有窗口，下次启动保留所选主题。"),
                ),
            Category::Updates => content
                .child(row(
                    "自动检查",
                    Select::new(&self.update_mode)
                        .w(px(240.0))
                        .into_any_element(),
                ))
                .when(prefs.mode == CheckMode::Interval, |content| {
                    content
                        .child(row(
                            "检查间隔",
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(Input::new(&self.interval).w(px(88.0)))
                                .child("小时")
                                .child(
                                    Button::new("prefs-save-interval")
                                        .label("保存间隔")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.save_interval(window, cx)
                                        })),
                                )
                                .into_any_element(),
                        ))
                        .child(
                            div()
                                .text_color(muted)
                                .child("允许 1–8760 小时，输入后按 Enter 或点击“保存间隔”。"),
                        )
                })
                .child(row(
                    "后台自动下载",
                    div()
                        .debug_selector(|| "prefs-auto-download".into())
                        .child(
                            Switch::new("prefs-auto-download")
                                .checked(prefs.auto_download)
                                .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                                    this.notice = crate::updates::set_auto_download(*enabled, cx)
                                        .err()
                                        .map(|error| format!("下载设置保存失败：{error:#}"))
                                        .unwrap_or_default();
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                ))
                .child(
                    div()
                        .text_color(muted)
                        .child("发现新版后后台下载，校验完成再弹出更新页。安装仍需确认。"),
                )
                .child(row(
                    "当前版本",
                    div()
                        .child(format!("Tiny MD {}", tiny_md_updater::CURRENT_VERSION))
                        .into_any_element(),
                ))
                .child(
                    Button::new("prefs-check-update")
                        .label("检查更新")
                        .w(px(120.0))
                        .on_click(|_, _, cx| crate::updates::show(cx, true, false)),
                ),
        };
        let error = &cx.global::<PreferencesState>().error;
        if !error.is_empty() {
            content = content.child(div().text_color(muted).child(error.clone()));
        }
        if !self.notice.is_empty() {
            content = content.child(div().text_color(muted).child(self.notice.clone()));
        }
        div()
            .id("preferences")
            .debug_selector(|| "preferences".into())
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .font_family(crate::ui_font())
            .text_size(px(13.0))
            .text_color(ink)
            .bg(surface)
            .child(titlebar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(sidebar)
                    .child(content),
            )
            .child(
                div()
                    .h(px(44.0))
                    .flex_shrink_0()
                    .px_6()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(quiet)
                    .border_t_1()
                    .border_color(border)
                    .child(div().text_color(muted).child("偏好设置保存在本机"))
                    .child(
                        div().debug_selector(|| "preferences-done".into()).child(
                            Button::new("preferences-done")
                                .label("完成")
                                .ghost()
                                .on_click(|_, window, _| window.remove_window()),
                        ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn appearance_preference_survives_restart_and_missing_files_keep_defaults() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("中文 app's $() & folder/preferences.json");
        assert!(!AppSettings::load(&path).unwrap().dark);
        AppSettings { dark: true }.save(&path).unwrap();
        assert!(AppSettings::load(&path).unwrap().dark);
    }

    #[test]
    fn malformed_preferences_are_reported_instead_of_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("preferences.json");
        std::fs::write(&path, b"invalid preferences").unwrap();
        assert!(AppSettings::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid preferences");
    }

    #[test]
    fn interval_input_rejects_partial_invalid_and_out_of_range_values() {
        for value in ["", "0", "8761", "-1", "2.5", "abc"] {
            assert!(parse_interval(value).is_err(), "accepted {value}");
        }
        assert_eq!(parse_interval(" 1 ").unwrap(), 1);
        assert_eq!(parse_interval("8760").unwrap(), 8760);
    }

    fn setup(cx: &mut TestAppContext, root: &Path) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::guise_theme(false).init(cx);
            cx.set_global(crate::Session::default());
            init_at(Some(root.join("preferences.json")), cx);
            crate::updates::init_at(Some(root.join("update-settings.json")), cx);
        });
    }

    #[gpui::test]
    fn opening_preferences_reuses_its_window_and_selects_the_requested_category(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        cx.update(|cx| {
            show(Category::Appearance, cx);
            let first = cx.global::<PreferencesState>().window.unwrap();
            show(Category::Updates, cx);
            assert!(cx.global::<PreferencesState>().window == Some(first));
            assert_eq!(cx.windows().len(), 1);
            let view = cx
                .global::<PreferencesState>()
                .view
                .as_ref()
                .unwrap()
                .upgrade()
                .unwrap();
            assert!(view.read(cx).category == Category::Updates);
        });
    }

    #[gpui::test]
    fn settings_controls_save_update_choices_and_reject_invalid_interval(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (view, cx) =
            cx.add_window_view(|window, cx| PreferencesView::new(Category::Updates, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        let toggle = cx
            .debug_bounds("prefs-auto-download")
            .expect("auto download switch");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            tiny_md_updater::Preferences::load(&root.path().join("update-settings.json"))
                .unwrap()
                .auto_download
        );
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.update_mode.update(cx, |_, cx| {
                    cx.emit(SelectEvent::<Vec<&'static str>>::Confirm(Some("按间隔")))
                });
                view.interval
                    .update(cx, |input, cx| input.set_value("72", window, cx));
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| view.update(cx, |view, cx| view.save_interval(window, cx)));
        let saved =
            tiny_md_updater::Preferences::load(&root.path().join("update-settings.json")).unwrap();
        assert_eq!(saved.mode, CheckMode::Interval);
        assert_eq!(saved.interval_hours, 72);
        assert!(
            saved.auto_download,
            "schedule edits preserve background download choice"
        );
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.interval
                    .update(cx, |input, cx| input.set_value("0", window, cx));
                view.save_interval(window, cx);
                assert!(view.notice.contains("未保存"));
            })
        });
        assert_eq!(
            tiny_md_updater::Preferences::load(&root.path().join("update-settings.json"))
                .unwrap()
                .interval_hours,
            72
        );
    }

    #[gpui::test]
    fn theme_settings_and_menu_share_persistence_without_changing_the_document(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (view, cx) = cx.add_window_view(|window, cx| crate::TinyMd::new(None, window, cx));
        cx.update(|window, cx| {
            cx.global_mut::<crate::Session>()
                .windows
                .push((window.window_handle(), view.downgrade()));
            view.update(cx, |view, cx| {
                view.install(
                    tiny_md_document::Document::untitled("unsaved draft"),
                    "unsaved draft",
                    window,
                    cx,
                );
                view.dirty = true;
                view.toggle_theme(window, cx);
                assert!(view.dark);
            });
        });
        cx.run_until_parked();
        assert!(
            AppSettings::load(&root.path().join("preferences.json"))
                .unwrap()
                .dark
        );
        cx.update(|window, cx| {
            save_theme(false, cx).unwrap();
            crate::apply_theme(false, window, cx);
        });
        cx.run_until_parked();
        cx.read(|cx| {
            assert!(!view.read(cx).dark);
            assert!(view.read(cx).dirty);
            assert_eq!(view.read(cx).editor.read(cx).text(), "unsaved draft");
        });
        assert!(
            !AppSettings::load(&root.path().join("preferences.json"))
                .unwrap()
                .dark
        );
    }

    #[gpui::test]
    fn preference_layout_keeps_controls_visible_at_minimum_window_size(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (view, cx) =
            cx.add_window_view(|window, cx| PreferencesView::new(Category::Appearance, window, cx));
        for dark in [false, true] {
            for category in [Category::Appearance, Category::Updates] {
                cx.update(|window, cx| {
                    crate::apply_theme(dark, window, cx);
                    view.update(cx, |view, _| view.category = category);
                });
                cx.simulate_resize(size(px(640.0), px(430.0)));
                cx.run_until_parked();
                for _ in 0..2 {
                    cx.update(|window, cx| {
                        window.refresh();
                        window.draw(cx).clear();
                    });
                }
                let done = cx
                    .debug_bounds("preferences-done")
                    .expect("visible preferences footer");
                assert!(done.origin.y >= px(0.0) && done.bottom() <= px(430.0));
                let category = cx.debug_bounds("prefs-updates").unwrap();
                assert!(category.origin.x >= px(0.0) && category.right() <= px(640.0));
            }
        }
    }
}
