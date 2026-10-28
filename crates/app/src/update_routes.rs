//! Route controls share the update center's live state across both utility windows.
use gpui::{prelude::*, *};
use gpui_component::{
    Disableable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    tooltip::Tooltip,
};
use tiny_md_updater::routes::{ProbeState, RouteMode, configured_routes};

pub(super) struct RouteControls {
    choice: Entity<SelectState<Vec<&'static str>>>,
    custom: Entity<InputState>,
    editing_custom: bool,
    notice: String,
    _subscriptions: Vec<Subscription>,
}
impl RouteControls {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prefs = crate::updates::preferences(cx);
        let choice = cx.new(|cx| {
            SelectState::new(
                RouteMode::ALL.map(RouteMode::label).to_vec(),
                Some(
                    gpui_component::IndexPath::default().row(
                        RouteMode::ALL
                            .iter()
                            .position(|m| *m == prefs.route_mode)
                            .unwrap(),
                    ),
                ),
                window,
                cx,
            )
        });
        let custom = cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/"));
        custom.update(cx, |input, cx| {
            input.set_value(prefs.custom_route, window, cx)
        });
        let subscriptions = vec![
            cx.observe(&crate::updates::center(cx), |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &choice,
                window,
                |this, _, event: &SelectEvent<Vec<&'static str>>, _, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        let Some(mode) = RouteMode::ALL.into_iter().find(|m| m.label() == *value)
                        else {
                            return;
                        };
                        this.editing_custom = mode == RouteMode::Custom;
                        this.notice.clear();
                        if mode != RouteMode::Custom {
                            let prefs = crate::updates::preferences(cx);
                            if let Err(error) =
                                crate::updates::set_route(mode, &prefs.custom_route, cx)
                            {
                                this.notice = error.to_string();
                            }
                        }
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(&custom, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.save_custom(cx);
                }
            }),
        ];
        Self {
            choice,
            custom,
            editing_custom: false,
            notice: String::new(),
            _subscriptions: subscriptions,
        }
    }
    fn save_custom(&mut self, cx: &mut Context<Self>) {
        let value = self.custom.read(cx).value().to_string();
        self.notice = match crate::updates::set_route(RouteMode::Custom, &value, cx) {
            Ok(()) => {
                self.editing_custom = false;
                "自定义线路已保存".into()
            }
            Err(error) => format!("线路未保存：{error}"),
        };
        cx.notify();
    }
}
impl Render for RouteControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = cx.global::<crate::Session>().dark;
        let muted = rgb(if dark { 0xb2b7bf } else { 0x787671 });
        let border = rgb(if dark { 0x34383f } else { 0xe5e3df });
        let quiet = rgb(if dark { 0x292d32 } else { 0xf8f7f5 });
        let good = rgb(if dark { 0x7cb98a } else { 0x398447 });
        let prefs = crate::updates::preferences(cx);
        let status = crate::updates::route_status(cx);
        let locked = crate::updates::routes_locked(cx);
        let mode = if self.editing_custom {
            RouteMode::Custom
        } else {
            prefs.route_mode
        };
        let label = mode.label();
        if self.choice.read(cx).selected_value() != Some(&label) {
            self.choice.update(cx, |choice, cx| {
                choice.set_selected_value(&label, window, cx)
            });
        }
        let routes = configured_routes(&prefs).unwrap_or_default();
        let mut list = div()
            .id("prefs-route-status")
            .debug_selector(|| "prefs-route-status".into())
            .flex()
            .flex_col()
            .w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(28.0))
                    .px_2()
                    .text_color(muted)
                    .border_b_1()
                    .border_color(border)
                    .child(div().flex_1().min_w_0().child("线路"))
                    .child(div().w(px(100.0)).flex_shrink_0().child("响应时间"))
                    .child(div().w(px(88.0)).flex_shrink_0().child("状态")),
            );
        for (index, route) in routes.iter().enumerate() {
            let result = status
                .report
                .entries
                .iter()
                .find(|entry| entry.route == *route);
            let active = status.active.as_ref() == Some(route);
            let valid = result.is_some_and(|r| r.state == ProbeState::Available);
            let time = if status.probing {
                "—".into()
            } else {
                result
                    .and_then(|r| r.elapsed_ms)
                    .map(|ms| format!("{ms} ms"))
                    .unwrap_or_else(|| "—".into())
            };
            let state = if status.probing {
                "检测中"
            } else if active {
                "当前使用"
            } else {
                result.map(|r| r.state.label()).unwrap_or("尚未检测")
            };
            let detail = result
                .map(|r| r.detail.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| route.label());
            list = list.child(
                div()
                    .id(("prefs-route", index))
                    .h(px(34.0))
                    .flex_shrink_0()
                    .px_2()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .when(active, |row| row.bg(quiet))
                    .tooltip(move |window, cx| Tooltip::new(detail.clone()).build(window, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(route.label()),
                    )
                    .child(div().w(px(100.0)).flex_shrink_0().child(time))
                    .child(
                        div()
                            .w(px(88.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(6.0)).rounded_full().bg(
                                if valid && !status.probing {
                                    good
                                } else {
                                    muted
                                },
                            ))
                            .child(state),
                    ),
            );
        }
        let freshness = match status.checked_at {
            _ if status.probing => "正在检测…".into(),
            Some(timestamp) => {
                let elapsed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(timestamp);
                if elapsed < 60 {
                    "刚刚检测".into()
                } else if elapsed < 3600 {
                    format!("{} 分钟前检测", elapsed / 60)
                } else {
                    format!("{} 小时前检测", elapsed / 3600)
                }
            }
            None => "尚未检测".into(),
        };
        let description = if self.editing_custom {
            "保存后使用自定义线路；未保存时仍使用原线路设置。"
        } else {
            match prefs.route_mode {
                RouteMode::Auto => "每次检查更新前检测线路，自动选择响应最快的可用线路。",
                RouteMode::Github => "检查和下载只连接 GitHub，不使用第三方线路。",
                _ => "使用指定下载线路；旧版无签名清单仍需 GitHub 直连核验。",
            }
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .border_t_1()
            .border_color(border)
            .pt_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .w(px(128.0))
                            .flex_shrink_0()
                            .font_weight(FontWeight::MEDIUM)
                            .child("下载线路"),
                    )
                    .child(
                        div()
                            .debug_selector(|| "prefs-update-route".into())
                            .child(Select::new(&self.choice).w(px(240.0)).disabled(locked)),
                    ),
            )
            .when(mode == RouteMode::Custom, |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Input::new(&self.custom).disabled(locked))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("prefs-save-route")
                                        .label("保存并使用")
                                        .disabled(locked)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.save_custom(cx)),
                                        ),
                                )
                                .when(self.editing_custom, |row| {
                                    row.child(
                                        Button::new("prefs-cancel-route")
                                            .label("取消")
                                            .ghost()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.editing_custom = false;
                                                this.notice.clear();
                                                cx.notify();
                                            })),
                                    )
                                }),
                        ),
                )
            })
            .child(
                div()
                    .text_color(muted)
                    .text_size(px(12.0))
                    .child(description),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().font_weight(FontWeight::MEDIUM).child("线路状态"))
                            .child(div().text_color(muted).text_size(px(12.0)).child(freshness)),
                    )
                    .child(
                        div().debug_selector(|| "prefs-probe-routes".into()).child(
                            Button::new("prefs-probe-routes")
                                .label(if status.probing {
                                    "正在测速…"
                                } else {
                                    "重新测速"
                                })
                                .disabled(status.probing || (locked && status.active.is_none()))
                                .on_click(|_, _, cx| crate::updates::probe_routes(cx)),
                        ),
                    ),
            )
            .child(list)
            .when(prefs.route_mode != RouteMode::Github, |panel| {
                panel.child(
                    div()
                        .text_color(muted)
                        .text_size(px(12.0))
                        .child("第三方线路可以获知你的 IP 和请求的更新文件。"),
                )
            })
            .when_some(status.active, |panel, route| {
                panel.child(
                    div()
                        .text_color(muted)
                        .text_size(px(12.0))
                        .child(format!("当前使用 {}", route.label())),
                )
            })
            .when(!self.notice.is_empty(), |panel| {
                panel.child(div().text_color(muted).child(self.notice.clone()))
            })
            .when(!status.message.is_empty(), |panel| {
                panel.child(div().text_color(muted).child(status.message))
            })
            .when_some(status.report.error, |panel, error| {
                panel.child(div().text_color(muted).text_size(px(12.0)).child(error))
            })
    }
}
