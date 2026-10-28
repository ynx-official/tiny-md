use crate::typography::{Typography, TypographyControls};
use anyhow::{Context as _, Result};
use gpui::{prelude::*, *};
use gpui_component::{
    Disableable, Root, TitleBar,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tiny_md_updater::CheckMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ThemePreference {
    Dark,
    Light,
    #[default]
    System,
}

impl ThemePreference {
    fn label(self) -> &'static str {
        match self {
            Self::Dark => "黑夜",
            Self::Light => "白天",
            Self::System => "跟随电脑",
        }
    }

    pub(super) fn is_dark(self, appearance: WindowAppearance) -> bool {
        match self {
            Self::Dark => true,
            Self::Light => false,
            Self::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(try_from = "StoredSettings")]
struct AppSettings {
    theme: ThemePreference,
    typography: Typography,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredSettings {
    theme: Option<ThemePreference>,
    dark: Option<bool>,
    typography: Typography,
}

impl TryFrom<StoredSettings> for AppSettings {
    type Error = anyhow::Error;

    fn try_from(stored: StoredSettings) -> Result<Self> {
        stored.typography.validate()?;
        // The old bool was only written after a manual choice. Retain it,
        // while missing settings follow the computer without writing a file.
        Ok(Self {
            theme: stored.theme.unwrap_or(match stored.dark {
                Some(true) => ThemePreference::Dark,
                Some(false) => ThemePreference::Light,
                None => ThemePreference::System,
            }),
            typography: stored.typography,
        })
    }
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
    let dark = settings.theme.is_dark(cx.window_appearance());
    cx.set_global(PreferencesState {
        settings,
        path,
        error,
        window: None,
        view: None,
    });
    dark
}

fn save_theme_preference(theme: ThemePreference, cx: &mut App) -> Result<()> {
    // Isolated document tests do not initialize the application settings store.
    if !cx.has_global::<PreferencesState>() {
        return Ok(());
    }
    let state = cx.global_mut::<PreferencesState>();
    let candidate = AppSettings {
        theme,
        ..state.settings.clone()
    };
    candidate.save(state.path.as_ref().context("无法找到配置目录")?)?;
    state.settings = candidate;
    state.error.clear();
    Ok(())
}

pub(super) fn theme_preference(cx: &App) -> ThemePreference {
    cx.try_global::<PreferencesState>()
        .map_or(ThemePreference::System, |state| state.settings.theme)
}

pub(super) fn typography(cx: &App) -> Typography {
    cx.try_global::<PreferencesState>()
        .map_or_else(Typography::default, |state| {
            state.settings.typography.clone()
        })
}

pub(super) fn set_typography(typography: Typography, cx: &mut App) -> Result<()> {
    typography.validate()?;
    let state = cx.global_mut::<PreferencesState>();
    let candidate = AppSettings {
        typography: typography.clone(),
        ..state.settings.clone()
    };
    candidate.save(state.path.as_ref().context("无法找到配置目录")?)?;
    state.settings = candidate;
    state.error.clear();
    let views = cx
        .try_global::<crate::Session>()
        .map(|session| {
            session
                .windows
                .iter()
                .map(|(_, view)| view.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    cx.defer(move |cx| {
        for view in views {
            let _ = view.update(cx, |view, cx| {
                view.font_size = typography.font_size as f32;
                view.editor.update(cx, |editor, cx| {
                    editor.set_style(typography.editor_style(view.dark), cx);
                    editor.set_font_size(view.font_size, cx);
                });
                cx.notify();
            });
        }
    });
    cx.refresh_windows();
    Ok(())
}

pub(super) fn set_theme(theme: ThemePreference, window: &mut Window, cx: &mut App) -> Result<()> {
    save_theme_preference(theme, cx)?;
    crate::apply_theme(theme.is_dark(window.appearance()), window, cx);
    Ok(())
}

pub(super) fn observe_appearance(window: &mut Window, cx: &mut App) {
    // The app can outlive its last window on macOS, so it may have missed an
    // OS notification. Resolve the new window's appearance before rendering.
    sync_appearance(window.appearance(), window, cx);
    window
        .observe_window_appearance(|window, cx| {
            sync_appearance(window.appearance(), window, cx);
        })
        .detach();
}

fn sync_appearance(appearance: WindowAppearance, window: &mut Window, cx: &mut App) {
    if !cx.has_global::<PreferencesState>() || !cx.has_global::<crate::Session>() {
        return;
    }
    let mode = theme_preference(cx);
    let dark = mode.is_dark(appearance);
    if mode == ThemePreference::System && cx.global::<crate::Session>().dark != dark {
        // OS notifications change the effective palette, not the saved choice.
        crate::apply_theme(dark, window, cx);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    Appearance,
    Typography,
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
    let bounds = Bounds::centered(None, size(px(900.0), px(700.0)), cx);
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
    typography: Entity<TypographyControls>,
    routes: Entity<crate::update_routes::RouteControls>,
    notice: String,
    _subscriptions: Vec<Subscription>,
}
impl PreferencesView {
    fn new(category: Category, window: &mut Window, cx: &mut Context<Self>) -> Self {
        observe_appearance(window, cx);
        let prefs = crate::updates::preferences(cx);
        let mode = theme_preference(cx);
        let modes = [
            ThemePreference::Dark,
            ThemePreference::Light,
            ThemePreference::System,
        ];
        let theme = cx.new(|cx| {
            SelectState::new(
                modes.map(ThemePreference::label).to_vec(),
                Some(
                    gpui_component::IndexPath::default()
                        .row(modes.iter().position(|item| *item == mode).unwrap()),
                ),
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
            cx.observe(&crate::updates::center(cx), |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &theme,
                window,
                |this, _, event: &SelectEvent<Vec<&'static str>>, window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        let mode = match *value {
                            "黑夜" => ThemePreference::Dark,
                            "白天" => ThemePreference::Light,
                            "跟随电脑" => ThemePreference::System,
                            _ => return,
                        };
                        match set_theme(mode, window, cx) {
                            Ok(()) => {
                                this.notice.clear();
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
            typography: cx.new(|cx| TypographyControls::new(window, cx)),
            routes: cx.new(|cx| crate::update_routes::RouteControls::new(window, cx)),
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
        let theme_label = theme_preference(cx).label();
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
            ("prefs-typography", "正文排版", Category::Typography),
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
            Category::Typography => "正文排版",
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
            .when(self.category == Category::Updates, |content| {
                content.gap_4()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(22.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(heading)
                    .when(self.category == Category::Updates, |header| {
                        header.child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(muted)
                                .child(format!("Tiny MD {}", tiny_md_updater::CURRENT_VERSION)),
                        )
                    }),
            );
        content = match self.category {
            Category::Typography => content.child(self.typography.clone()),
            Category::Appearance => content
                .child(row(
                    "夜间模式",
                    div()
                        .id("prefs-night-mode")
                        .debug_selector(|| "prefs-night-mode".into())
                        .child(Select::new(&self.theme).w(px(240.0)))
                        .into_any_element(),
                ))
                .child(
                    div()
                        .text_color(muted)
                        .child("应用于所有窗口并记住所选模式；跟随电脑时自动切换黑夜与白天。"),
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
                .child(self.routes.clone())
                .child(
                    div().debug_selector(|| "prefs-check-update".into()).child(
                        Button::new("prefs-check-update")
                            .label("检查更新")
                            .disabled(crate::updates::routes_locked(cx))
                            .w(px(120.0))
                            .on_click(|_, _, cx| crate::updates::show(cx, true, false)),
                    ),
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
    fn typography_preferences_survive_restart_and_theme_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("preferences.json");
        let typography = serde_json::json!({
            "content_width": 1140,
            "font_family": "Microsoft YaHei",
            "font_size": 18,
            "line_height": 1.8,
            "paragraph_spacing": 0.75
        });
        let settings: AppSettings = serde_json::from_value(serde_json::json!({
            "theme": "dark", "typography": typography
        }))
        .unwrap();
        settings.save(&path).unwrap();
        let saved = serde_json::to_value(AppSettings::load(&path).unwrap()).unwrap();
        assert_eq!(saved["typography"], typography);
    }

    #[test]
    fn typography_rejects_out_of_range_saved_values() {
        for (field, value) in [
            ("content_width", serde_json::json!(0)),
            ("content_width", serde_json::json!(2001)),
            ("font_size", serde_json::json!(31)),
            ("line_height", serde_json::json!(0.0)),
            ("paragraph_spacing", serde_json::json!(-1.0)),
            ("font_family", serde_json::json!("bad\nfont")),
        ] {
            let mut typography = serde_json::Map::new();
            typography.insert(field.into(), value);
            assert!(
                serde_json::from_value::<AppSettings>(serde_json::json!({
                    "typography": typography
                }))
                .is_err(),
                "invalid {field} accepted"
            );
        }
    }

    #[gpui::test]
    fn typography_category_is_available_from_preferences(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (_, cx) =
            cx.add_window_view(|window, cx| PreferencesView::new(Category::Appearance, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
        assert!(
            cx.debug_bounds("prefs-typography").is_some(),
            "正文排版入口应出现在偏好设置分类中"
        );
    }

    #[gpui::test]
    fn typography_new_documents_and_zoom_reset_use_saved_size(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("preferences.json");
        std::fs::write(&path, r#"{"typography":{"font_size":18}}"#).unwrap();
        setup(cx, root.path());
        let (view, cx) = cx.add_window_view(|window, cx| crate::TinyMd::new(None, window, cx));
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.font_size, 18.0);
                view.zoom(2.0, cx);
                assert_eq!(view.font_size, 20.0);
                view.zoom(0.0, cx);
                assert_eq!(view.font_size, 18.0);
                assert!(!view.dirty);
            })
        });
    }

    #[gpui::test]
    fn typography_applies_to_open_windows_preserves_theme_and_failed_save(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        cx.update(|cx| {
            crate::open_editor_window(None, true, cx);
            crate::open_editor_window(None, true, cx);
        });
        cx.run_until_parked();
        let custom = Typography {
            content_width: 1140,
            font_size: 18,
            line_height: 2.0,
            paragraph_spacing: 0.75,
            ..Default::default()
        };
        cx.update(|cx| {
            save_theme_preference(ThemePreference::Dark, cx).unwrap();
            set_typography(custom.clone(), cx).unwrap();
        });
        cx.run_until_parked();
        cx.read(|cx| {
            assert_eq!(cx.global::<crate::Session>().windows.len(), 2);
            for (_, view) in &cx.global::<crate::Session>().windows {
                let view = view.upgrade().unwrap();
                assert_eq!(view.read(cx).font_size, 18.0);
                assert!(!view.read(cx).dirty);
                assert_eq!(view.read(cx).editor.read(cx).text(), "");
            }
        });
        cx.update(|cx| save_theme_preference(ThemePreference::Light, cx).unwrap());
        let saved = AppSettings::load(&root.path().join("preferences.json")).unwrap();
        assert_eq!(saved.typography, custom);
        assert_eq!(saved.theme, ThemePreference::Light);
        cx.update(|cx| {
            let invalid = root.path().join("directory");
            std::fs::create_dir(&invalid).unwrap();
            cx.global_mut::<PreferencesState>().path = Some(invalid);
            assert!(set_typography(Typography::default(), cx).is_err());
            assert_eq!(typography(cx), custom);
            assert_eq!(theme_preference(cx), ThemePreference::Light);
        });
    }

    #[test]
    fn appearance_preference_survives_restart_and_missing_files_keep_defaults() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("中文 app's $() & folder/preferences.json");
        assert_eq!(
            AppSettings::load(&path).unwrap().theme,
            ThemePreference::System
        );
        assert!(!path.exists());
        for theme in [
            ThemePreference::Dark,
            ThemePreference::Light,
            ThemePreference::System,
        ] {
            AppSettings {
                theme,
                ..Default::default()
            }
            .save(&path)
            .unwrap();
            assert_eq!(AppSettings::load(&path).unwrap().theme, theme);
        }
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
    fn night_mode_migrates_explicit_legacy_choices_and_resolves_system_appearance() {
        for (json, expected) in [
            (r#"{}"#, ThemePreference::System),
            (r#"{"dark":true}"#, ThemePreference::Dark),
            (r#"{"dark":false}"#, ThemePreference::Light),
            (r#"{"theme":"system","dark":true}"#, ThemePreference::System),
        ] {
            let settings: AppSettings = serde_json::from_str(json).unwrap();
            assert_eq!(settings.theme, expected);
            let saved = serde_json::to_value(settings).unwrap();
            assert!(saved.get("theme").is_some());
            assert!(saved.get("dark").is_none());
        }
        assert!(serde_json::from_str::<AppSettings>(r#"{"theme":"unknown"}"#).is_err());
        for appearance in [
            WindowAppearance::Light,
            WindowAppearance::VibrantLight,
            WindowAppearance::Dark,
            WindowAppearance::VibrantDark,
        ] {
            assert!(ThemePreference::Dark.is_dark(appearance));
            assert!(!ThemePreference::Light.is_dark(appearance));
            assert_eq!(
                ThemePreference::System.is_dark(appearance),
                matches!(
                    appearance,
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                )
            );
        }
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
    fn night_mode_defaults_to_following_computer(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (view, cx) =
            cx.add_window_view(|window, cx| PreferencesView::new(Category::Appearance, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
        cx.read(|cx| {
            assert_eq!(
                view.read(cx).theme.read(cx).selected_value(),
                Some(&"跟随电脑")
            );
        });
    }

    #[gpui::test]
    fn night_mode_new_window_resolves_current_system_appearance(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        cx.update(|cx| cx.global_mut::<crate::Session>().dark = true);
        let (view, cx) = cx.add_window_view(|window, cx| crate::TinyMd::new(None, window, cx));
        cx.update(|window, cx| {
            let expected = ThemePreference::System.is_dark(window.appearance());
            assert_eq!(view.read(cx).dark, expected);
            assert_eq!(cx.global::<crate::Session>().dark, expected);
        });
        assert!(!root.path().join("preferences.json").exists());
    }

    #[gpui::test]
    fn night_mode_controls_persist_all_choices_and_system_events_keep_the_preference(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        cx.update(|cx| crate::open_editor_window(None, true, cx));
        let (prefs, cx) =
            cx.add_window_view(|window, cx| PreferencesView::new(Category::Appearance, window, cx));
        cx.run_until_parked();
        for (label, mode) in [
            ("黑夜", ThemePreference::Dark),
            ("白天", ThemePreference::Light),
            ("跟随电脑", ThemePreference::System),
        ] {
            prefs.update(cx, |prefs, cx| {
                prefs.theme.update(cx, |_, cx| {
                    cx.emit(SelectEvent::<Vec<&'static str>>::Confirm(Some(label)));
                })
            });
            cx.run_until_parked();
            assert_eq!(
                AppSettings::load(&root.path().join("preferences.json"))
                    .unwrap()
                    .theme,
                mode
            );
            for appearance in [WindowAppearance::Dark, WindowAppearance::Light] {
                cx.update(|window, cx| sync_appearance(appearance, window, cx));
                cx.run_until_parked();
                cx.update(|window, cx| {
                    window.draw(cx).clear();
                });
                cx.read(|cx| {
                    let expected = mode.is_dark(appearance);
                    assert_eq!(cx.global::<crate::Session>().dark, expected);
                    assert_eq!(gpui_component::Theme::global(cx).is_dark(), expected);
                    assert_eq!(guise::theme::theme(cx).scheme.is_dark(), expected);
                    for (_, doc) in &cx.global::<crate::Session>().windows {
                        let doc = doc.upgrade().unwrap();
                        assert_eq!(doc.read(cx).dark, expected);
                        assert_eq!(doc.read(cx).editor.read(cx).text(), "");
                        assert!(!doc.read(cx).dirty);
                    }
                    assert_eq!(prefs.read(cx).theme.read(cx).selected_value(), Some(&label));
                });
            }
            assert_eq!(
                AppSettings::load(&root.path().join("preferences.json"))
                    .unwrap()
                    .theme,
                mode
            );
        }
    }

    #[gpui::test]
    fn night_mode_manual_choice_overrides_system_even_when_palette_matches_and_failed_save_is_atomic(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (view, cx) = cx.add_window_view(|window, cx| crate::TinyMd::new(None, window, cx));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.select_theme(ThemePreference::Light, window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(theme_preference(cx), ThemePreference::Light);
            sync_appearance(WindowAppearance::Dark, window, cx);
            assert!(!cx.global::<crate::Session>().dark);
            // A directory at the destination reliably fails atomic file replacement.
            let invalid = root.path().join("directory");
            std::fs::create_dir(&invalid).unwrap();
            cx.global_mut::<PreferencesState>().path = Some(invalid);
            assert!(set_theme(ThemePreference::Dark, window, cx).is_err());
            assert_eq!(theme_preference(cx), ThemePreference::Light);
            assert!(!cx.global::<crate::Session>().dark);
        });
        assert_eq!(
            AppSettings::load(&root.path().join("preferences.json"))
                .unwrap()
                .theme,
            ThemePreference::Light
        );
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
    fn update_route_controls_are_visible_in_the_approved_layout(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (_, cx) =
            cx.add_window_view(|window, cx| PreferencesView::new(Category::Updates, window, cx));
        cx.simulate_resize(size(px(900.0), px(700.0)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        assert!(
            cx.debug_bounds("prefs-update-route").is_some(),
            "route selector is visible"
        );
        assert!(
            cx.debug_bounds("prefs-probe-routes").is_some(),
            "manual probe is visible"
        );
        assert!(
            cx.debug_bounds("prefs-route-status").is_some(),
            "route status list is visible"
        );
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
        assert_eq!(
            AppSettings::load(&root.path().join("preferences.json"))
                .unwrap()
                .theme,
            ThemePreference::Dark
        );
        cx.update(|window, cx| {
            set_theme(ThemePreference::Light, window, cx).unwrap();
        });
        cx.run_until_parked();
        cx.read(|cx| {
            assert!(!view.read(cx).dark);
            assert!(view.read(cx).dirty);
            assert_eq!(view.read(cx).editor.read(cx).text(), "unsaved draft");
        });
        assert_eq!(
            AppSettings::load(&root.path().join("preferences.json"))
                .unwrap()
                .theme,
            ThemePreference::Light
        );
    }

    #[gpui::test]
    fn preference_layout_keeps_controls_visible_at_minimum_window_size(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        setup(cx, root.path());
        let (view, cx) =
            cx.add_window_view(|window, cx| PreferencesView::new(Category::Appearance, window, cx));
        for dark in [false, true] {
            for category in [
                Category::Appearance,
                Category::Typography,
                Category::Updates,
            ] {
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
                if cx.read(|cx| view.read(cx).category == Category::Updates) {
                    let viewport = cx.debug_bounds("preferences-content").unwrap();
                    let route = cx.debug_bounds("prefs-update-route").unwrap();
                    assert!(route.left() >= viewport.left() && route.right() <= viewport.right());
                    cx.simulate_event(ScrollWheelEvent {
                        position: viewport.center(),
                        delta: ScrollDelta::Pixels(point(px(0.0), px(-1000.0))),
                        ..Default::default()
                    });
                    cx.run_until_parked();
                    cx.update(|window, cx| {
                        window.draw(cx).clear();
                    });
                    let check = cx.debug_bounds("prefs-check-update").unwrap();
                    assert!(
                        check.top() >= viewport.top() && check.bottom() <= viewport.bottom(),
                        "check updates remains reachable by scrolling at minimum size"
                    );
                }
                if cx.read(|cx| view.read(cx).category == Category::Typography) {
                    let viewport = cx.debug_bounds("preferences-content").unwrap();
                    let font = cx.debug_bounds("typography-font").unwrap();
                    assert!(font.left() >= viewport.left() && font.right() <= viewport.right());
                    cx.simulate_mouse_move(viewport.center(), None, Modifiers::none());
                    cx.simulate_event(ScrollWheelEvent {
                        position: viewport.center(),
                        delta: ScrollDelta::Pixels(point(px(0.0), px(-1000.0))),
                        ..Default::default()
                    });
                    cx.run_until_parked();
                    cx.update(|window, cx| {
                        window.draw(cx).clear();
                    });
                    let save = cx.debug_bounds("typography-save").unwrap();
                    assert!(
                        save.top() >= viewport.top() && save.bottom() <= viewport.bottom(),
                        "save remains reachable by scrolling at minimum size"
                    );
                }
            }
        }
    }
}
