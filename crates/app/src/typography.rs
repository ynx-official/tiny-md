use anyhow::{Context as _, Result, ensure};
use gpui::{prelude::*, *};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    select::{SearchableVec, Select, SelectState},
};
use serde::{Deserialize, Serialize};

const DEFAULT_FONT: &str = "系统默认";

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Typography {
    /// Width of the text itself, excluding the editor's horizontal padding.
    pub content_width: u32,
    pub font_family: String,
    pub font_size: u32,
    pub line_height: f64,
    pub paragraph_spacing: f64,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            content_width: 1140,
            font_family: String::new(),
            font_size: 16,
            line_height: 1.6,
            paragraph_spacing: 0.5,
        }
    }
}

impl Typography {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (480..=2000).contains(&self.content_width),
            "正文宽度应为 480–2000 的整数"
        );
        ensure!(
            (12..=30).contains(&self.font_size),
            "正文字号应为 12–30 的整数"
        );
        ensure!(
            (1.2..=2.4).contains(&self.line_height),
            "正文行高应为 1.2–2.4 倍字号"
        );
        ensure!(
            (0.25..=2.0).contains(&self.paragraph_spacing),
            "段落间距应为 0.25–2 行"
        );
        ensure!(
            self.font_family.len() <= 256 && !self.font_family.chars().any(char::is_control),
            "字体名称无效"
        );
        Ok(())
    }

    pub fn font_family(&self) -> SharedString {
        if self.font_family.is_empty() {
            crate::ui_font().into()
        } else {
            self.font_family.clone().into()
        }
    }

    pub fn editor_style(&self, dark: bool) -> tiny_md_editor::MarkdownStyle {
        tiny_md_editor::MarkdownStyle {
            line_height: Some(self.line_height as f32),
            paragraph_spacing: Some(self.paragraph_spacing as f32),
            ..crate::editor_style(dark)
        }
    }

    fn from_inputs(font: &str, values: [&str; 4]) -> Result<Self> {
        let result = Self {
            content_width: values[0]
                .trim()
                .parse()
                .context("正文宽度应为 480–2000 的整数")?,
            font_size: values[1]
                .trim()
                .parse()
                .context("正文字号应为 12–30 的整数")?,
            line_height: values[2]
                .trim()
                .parse()
                .context("正文行高应为 1.2–2.4 的数字")?,
            paragraph_spacing: values[3]
                .trim()
                .parse()
                .context("段落间距应为 0.25–2 的数字")?,
            font_family: if font == DEFAULT_FONT {
                String::new()
            } else {
                font.to_string()
            },
        };
        result.validate()?;
        Ok(result)
    }

    fn values(&self) -> [String; 4] {
        [
            self.content_width.to_string(),
            self.font_size.to_string(),
            self.line_height.to_string(),
            self.paragraph_spacing.to_string(),
        ]
    }
}

pub(super) struct TypographyControls {
    font: Entity<SelectState<SearchableVec<String>>>,
    fonts: Vec<String>,
    inputs: [Entity<InputState>; 4],
    notice: String,
    error: bool,
    _subscriptions: Vec<Subscription>,
}

impl TypographyControls {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = crate::preferences::typography(cx);
        let mut fonts = window.text_system().all_font_names();
        // Preserve an unavailable saved family so opening settings never silently replaces it.
        if !settings.font_family.is_empty() {
            fonts.push(settings.font_family.clone());
        }
        fonts.retain(|name| !name.is_empty() && name != DEFAULT_FONT);
        fonts.sort_unstable();
        fonts.dedup();
        fonts.insert(0, DEFAULT_FONT.into());
        let selected = fonts
            .iter()
            .position(|name| *name == settings.font_family)
            .unwrap_or(0);
        let font = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(fonts.clone()),
                Some(gpui_component::IndexPath::default().row(selected)),
                window,
                cx,
            )
            .searchable(true)
        });
        let inputs = settings
            .values()
            .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value)));
        let subscriptions = inputs
            .iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.save(window, cx);
                    }
                })
            })
            .collect();
        Self {
            font,
            fonts,
            inputs,
            notice: String::new(),
            error: false,
            _subscriptions: subscriptions,
        }
    }

    fn save(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let font = self
            .font
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| DEFAULT_FONT.into());
        let values = self.inputs.each_ref().map(|input| input.read(cx).value());
        let result = Typography::from_inputs(&font, values.each_ref().map(|value| value.as_ref()))
            .and_then(|settings| crate::preferences::set_typography(settings, cx));
        self.report(result, "排版已保存并应用到所有文档", cx);
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = Typography::default();
        let result = crate::preferences::set_typography(settings.clone(), cx);
        if result.is_ok() {
            for (input, value) in self.inputs.iter().zip(settings.values()) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
            // A prior search may have filtered out the default item. Reset the
            // search as well as the selected value, without re-enumerating fonts.
            self.font = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(self.fonts.clone()),
                    Some(gpui_component::IndexPath::default().row(0)),
                    window,
                    cx,
                )
                .searchable(true)
            });
        }
        self.report(result, "已恢复默认排版", cx);
    }

    fn report(&mut self, result: Result<()>, success: &str, cx: &mut Context<Self>) {
        self.error = result.is_err();
        self.notice =
            result.map_or_else(|error| format!("排版未保存：{error:#}"), |_| success.into());
        cx.notify();
    }
}

impl Render for TypographyControls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = cx.global::<crate::Session>().dark;
        let muted = rgb(if dark { 0xb2b7bf } else { 0x787671 });
        let row = |label: &'static str, control: AnyElement, hint: &'static str| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .flex_shrink_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .w(px(100.0))
                                .flex_shrink_0()
                                .font_weight(FontWeight::MEDIUM)
                                .child(label),
                        )
                        .child(div().flex_1().min_w_0().child(control)),
                )
                .child(div().text_size(px(12.0)).text_color(muted).child(hint))
        };
        let mut content = div()
            .flex()
            .flex_col()
            .gap_4()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .text_color(muted)
                    .child("输入后按 Enter 或点击“保存并应用”，所有文档同步生效。"),
            )
            .child(row(
                "正文字体",
                div()
                    .debug_selector(|| "typography-font".into())
                    .child(Select::new(&self.font).w_full())
                    .into_any_element(),
                "可搜索本机字体；系统默认沿用原字体。代码保持等宽。",
            ));
        for (index, (id, label, unit, hint)) in [
            (
                "typography-width",
                "正文宽度",
                "px",
                "480–2000；文字区域的最大宽度，窗口较窄时自动收窄。",
            ),
            (
                "typography-size",
                "正文字号",
                "px",
                "12–30；标题按比例缩放，“实际大小”恢复此字号。",
            ),
            (
                "typography-line-height",
                "正文行高",
                "倍字号",
                "1.2–2.4；调整正文、列表和引用的行高。",
            ),
            (
                "typography-paragraph",
                "段落间距",
                "行",
                "0.25–2；调整段落间的单个空行，连续空行保留。",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            content = content.child(row(
                label,
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .debug_selector(move || id.into())
                            .child(Input::new(&self.inputs[index]).w(px(88.0))),
                    )
                    .child(unit)
                    .into_any_element(),
                hint,
            ));
        }
        content
            .child(
                div()
                    .flex()
                    .gap_2()
                    .flex_wrap()
                    .flex_shrink_0()
                    .child(
                        div().debug_selector(|| "typography-save".into()).child(
                            Button::new("typography-save")
                                .primary()
                                .label("保存并应用")
                                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                        ),
                    )
                    .child(
                        div().debug_selector(|| "typography-reset".into()).child(
                            Button::new("typography-reset").label("恢复默认").on_click(
                                cx.listener(|this, _, window, cx| this.reset(window, cx)),
                            ),
                        ),
                    ),
            )
            .when(!self.notice.is_empty(), |content| {
                content.child(
                    div()
                        .text_color(if self.error {
                            rgb(if dark { 0xffa19a } else { 0xb42318 })
                        } else {
                            muted
                        })
                        .child(self.notice.clone()),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn typography_default_text_width_matches_requested_1140() {
        assert_eq!(Typography::default().content_width, 1140);
    }

    #[test]
    fn numeric_inputs_require_complete_finite_values_in_range() {
        for (index, bad) in [
            (0, ""),
            (0, "480.5"),
            (0, "2001"),
            (1, "11"),
            (1, "30.1"),
            (2, "NaN"),
            (2, "inf"),
            (2, "0"),
            (3, "NaN"),
            (3, "0"),
            (3, "2.1"),
        ] {
            let mut values = ["1140", "18", "1.8", "0.75"];
            values[index] = bad;
            assert!(
                Typography::from_inputs(DEFAULT_FONT, values).is_err(),
                "accepted {bad} in field {index}"
            );
        }
        assert!(Typography::from_inputs(DEFAULT_FONT, ["480", "12", "1.2", "0.25"]).is_ok());
        assert!(Typography::from_inputs("Microsoft YaHei", [" 2000 ", "30", "2.4", "2"]).is_ok());
    }

    #[gpui::test]
    fn typography_controls_save_on_enter_reject_invalid_input_and_reset(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        crate::file_open_tests::init(cx);
        cx.update(|cx| {
            crate::preferences::init_at(Some(root.path().join("preferences.json")), cx);
        });
        let (view, cx) = cx.add_window_view(TypographyControls::new);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                for (input, value) in view.inputs.iter().zip(["1140", "18", "1.8", "0.75"]) {
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                }
                view.inputs[0].update(cx, |_, cx| {
                    cx.emit(InputEvent::PressEnter { secondary: false })
                });
            })
        });
        cx.run_until_parked();
        cx.read(|cx| {
            let saved = crate::preferences::typography(cx);
            assert_eq!(saved.content_width, 1140);
            assert_eq!(saved.font_size, 18);
            assert_eq!(saved.line_height, 1.8);
            assert_eq!(saved.paragraph_spacing, 0.75);
            assert!(!view.read(cx).error);
        });
        let bytes = std::fs::read(root.path().join("preferences.json")).unwrap();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.inputs[2].update(cx, |input, cx| input.set_value("NaN", window, cx));
                view.save(window, cx);
                assert!(view.error);
            })
        });
        assert_eq!(
            std::fs::read(root.path().join("preferences.json")).unwrap(),
            bytes
        );
        cx.update(|window, cx| view.update(cx, |view, cx| view.reset(window, cx)));
        cx.run_until_parked();
        cx.read(|cx| assert_eq!(crate::preferences::typography(cx), Typography::default()));
    }
}
