//! Route controls share the update center's live state across both utility windows.
use gpui::{prelude::*, *};
use gpui_component::{
    Disableable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    tooltip::Tooltip,
};
use tiny_md_updater::{
    Preferences,
    routes::{ProbeState, RouteMode, configured_routes},
};

const ADD_CUSTOM: &str = "添加自定义线路…";

fn choice_items(prefs: &Preferences) -> Vec<String> {
    RouteMode::ALL
        .into_iter()
        .filter(|mode| *mode != RouteMode::Custom)
        .map(|mode| mode.label().to_string())
        .chain(prefs.custom_routes.iter().cloned())
        .chain(std::iter::once(ADD_CUSTOM.to_string()))
        .collect()
}

fn selected_choice(prefs: &Preferences) -> String {
    if prefs.route_mode == RouteMode::Custom {
        prefs.custom_route.clone()
    } else {
        prefs.route_mode.label().to_string()
    }
}

pub(super) struct RouteControls {
    choice: Entity<SelectState<Vec<String>>>,
    choices: Vec<String>,
    custom: Entity<InputState>,
    editing_custom: bool,
    notice: String,
    _subscriptions: Vec<Subscription>,
}
impl RouteControls {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prefs = crate::updates::preferences(cx);
        let choices = choice_items(&prefs);
        let choice = cx.new(|cx| SelectState::new(choices.clone(), None, window, cx));
        choice.update(cx, |choice, cx| {
            choice.set_selected_value(&selected_choice(&prefs), window, cx)
        });
        let custom = cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/"));
        let subscriptions = vec![
            cx.observe(&crate::updates::center(cx), |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &choice,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        if value == ADD_CUSTOM {
                            this.begin_add(window, cx);
                            return;
                        }
                        this.notice.clear();
                        let mode = RouteMode::ALL
                            .into_iter()
                            .find(|m| m.label() == value)
                            .unwrap_or(RouteMode::Custom);
                        if let Err(error) = crate::updates::set_route(mode, value, cx) {
                            this.notice = error.to_string();
                        }
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &custom,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.save_custom(window, cx);
                    }
                },
            ),
        ];
        Self {
            choice,
            choices,
            custom,
            editing_custom: false,
            notice: String::new(),
            _subscriptions: subscriptions,
        }
    }
    fn begin_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_custom = true;
        self.notice.clear();
        self.custom.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }
    fn save_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.custom.read(cx).value().to_string();
        self.notice = match crate::updates::add_custom_route(&value, cx) {
            Ok(()) => {
                self.custom.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                });
                "线路已添加，可继续新增；在下载线路中可单独选用。".into()
            }
            Err(error) => format!("线路未保存：{error}"),
        };
        cx.notify();
    }
    fn remove_custom(&mut self, prefix: &str, cx: &mut Context<Self>) {
        let prefs = crate::updates::preferences(cx);
        let selected = prefs.route_mode == RouteMode::Custom && prefs.custom_route == prefix;
        self.notice = match crate::updates::remove_custom_route(prefix, cx) {
            Ok(()) if selected => "已移除指定线路，已切换为自动择优。".into(),
            Ok(()) => "自定义线路已移除。".into(),
            Err(error) => format!("线路未移除：{error}"),
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
        let label = selected_choice(&prefs);
        let choices = choice_items(&prefs);
        if self.choices != choices || self.choice.read(cx).selected_value() != Some(&label) {
            self.choices = choices.clone();
            self.choice.update(cx, |choice, cx| {
                choice.set_items(choices, window, cx);
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
        let description = match prefs.route_mode {
            RouteMode::Auto => "每次检查更新前检测线路，自动选择响应最快的可用线路。",
            RouteMode::Github => "检查和下载只连接 GitHub，不使用第三方线路。",
            _ => "使用指定下载线路；旧版无签名清单仍需 GitHub 直连核验。",
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
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("自定义线路（{}）", prefs.custom_routes.len())),
                    )
                    .child(
                        div().debug_selector(|| "prefs-add-route".into()).child(
                            Button::new("prefs-add-route")
                                .label("新增线路")
                                .ghost()
                                .disabled(locked)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.begin_add(window, cx)),
                                ),
                        ),
                    ),
            )
            .when(
                prefs.custom_routes.is_empty() && !self.editing_custom,
                |panel| {
                    panel.child(
                        div()
                            .text_color(muted)
                            .text_size(px(12.0))
                            .child("尚未添加。可逐条添加 HTTPS 代理，自动择优时一并检测。"),
                    )
                },
            )
            .children(
                prefs
                    .custom_routes
                    .iter()
                    .enumerate()
                    .map(|(index, prefix)| {
                        let prefix = prefix.clone();
                        let tooltip = prefix.clone();
                        let remove = prefix.clone();
                        div()
                            .id(("prefs-custom-route", index))
                            .flex()
                            .items_center()
                            .gap_2()
                            .debug_selector(move || format!("prefs-custom-route-{index}"))
                            .child(
                                div()
                                    .id(("prefs-custom-prefix", index))
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(tooltip.clone()).build(window, cx)
                                    })
                                    .child(prefix),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .debug_selector(move || format!("prefs-remove-route-{index}"))
                                    .child(
                                        Button::new(("prefs-remove-route", index))
                                            .label("移除")
                                            .ghost()
                                            .disabled(locked)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.remove_custom(&remove, cx)
                                            })),
                                    ),
                            )
                    }),
            )
            .when(!self.notice.is_empty(), |panel| {
                panel.child(
                    div()
                        .text_color(muted)
                        .text_size(px(12.0))
                        .child(self.notice.clone()),
                )
            })
            .when(self.editing_custom, |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .debug_selector(|| "prefs-custom-input".into())
                                .child(Input::new(&self.custom).disabled(locked)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div().debug_selector(|| "prefs-save-route".into()).child(
                                        Button::new("prefs-save-route")
                                            .label("添加")
                                            .disabled(locked)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save_custom(window, cx)
                                            })),
                                    ),
                                )
                                .when(self.editing_custom, |row| {
                                    row.child(
                                        Button::new("prefs-cancel-route")
                                            .label("取消")
                                            .ghost()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.editing_custom = false;
                                                this.notice.clear();
                                                this.custom.update(cx, |input, cx| {
                                                    input.set_value("", window, cx)
                                                });
                                                cx.notify();
                                            })),
                                    )
                                }),
                        ),
                )
            })
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
            .when(!status.message.is_empty(), |panel| {
                panel.child(div().text_color(muted).child(status.message))
            })
            .when_some(status.report.error, |panel, error| {
                panel.child(div().text_color(muted).text_size(px(12.0)).child(error))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[gpui::test]
    fn controls_add_multiple_routes_select_and_remove_each_without_resurrection(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        crate::file_open_tests::init(cx);
        cx.update(|cx| crate::updates::init_at(Some(path.clone()), cx));
        let mut controls = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| RouteControls::new(window, cx));
            controls = Some(view.clone());
            gpui_component::Root::new(view, window, cx)
        });
        let view = controls.unwrap();
        cx.simulate_resize(size(px(640.0), px(900.0)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
        let add = cx.debug_bounds("prefs-add-route").unwrap();
        cx.simulate_click(add.center(), Modifiers::none());
        cx.run_until_parked();
        for suffix in ["one", "two"] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    assert!(view.editing_custom);
                    view.custom.update(cx, |input, cx| {
                        input.set_value(format!("https://mirror.example/{suffix}"), window, cx);
                    });
                });
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.draw(cx).clear();
            });
            if suffix == "one" {
                let save = cx.debug_bounds("prefs-save-route").unwrap();
                cx.simulate_click(save.center(), Modifiers::none());
            } else {
                cx.update(|_, cx| {
                    view.read(cx).custom.clone().update(cx, |_, cx| {
                        cx.emit(InputEvent::PressEnter { secondary: false });
                    })
                });
            }
            cx.run_until_parked();
            cx.read(|cx| assert!(view.read(cx).custom.read(cx).value().is_empty()));
        }
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.custom_routes.len(), 2);
        assert_eq!(loaded.route_mode, RouteMode::Auto);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.choice.update(cx, |_, cx| {
                    cx.emit(SelectEvent::<Vec<String>>::Confirm(Some(
                        "https://mirror.example/two/".into(),
                    )));
                });
            })
        });
        cx.run_until_parked();
        assert_eq!(
            Preferences::load(&path).unwrap().route_mode,
            RouteMode::Custom
        );
        for _ in 0..2 {
            cx.update(|window, cx| {
                window.draw(cx).clear();
            });
            let button = cx.debug_bounds("prefs-remove-route-0").unwrap();
            cx.simulate_click(button.center(), Modifiers::none());
            cx.run_until_parked();
        }
        let loaded = Preferences::load(&path).unwrap();
        assert!(loaded.custom_routes.is_empty());
        assert_eq!(loaded.route_mode, RouteMode::Auto);
        cx.read(|cx| {
            assert!(
                !view
                    .read(cx)
                    .choices
                    .iter()
                    .any(|label| label.starts_with("https://"))
            );
            assert_eq!(
                view.read(cx).choice.read(cx).selected_value().unwrap(),
                "自动择优"
            );
        });
    }
}
