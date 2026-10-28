//! Size drafts stay outside the document until the user applies them.
use super::*;
use guise::{TextInput, TextInputEvent};

gpui::actions!(
    tiny_md_table_size,
    [NextField, PreviousField, ApplySize, CancelSize]
);

pub(super) struct SizePicker {
    columns: Entity<TextInput>,
    rows: Entity<TextInput>,
    preview: Option<(usize, usize)>,
    loss: Option<usize>,
    pending: Option<(usize, usize)>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl SizePicker {
    pub(super) fn new(
        cell: TableContext,
        window: &mut Window,
        cx: &mut Context<TableMenu>,
    ) -> Self {
        let columns = cx.new(|cx| {
            TextInput::new(cx)
                .value(&cell.columns.to_string())
                .size(guise::theme::Size::Xs)
        });
        let rows = cx.new(|cx| {
            TextInput::new(cx)
                .value(&cell.rows.to_string())
                .size(guise::theme::Size::Xs)
        });
        let subscriptions = [&columns, &rows]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(
                    input,
                    window,
                    |this, _, event: &TextInputEvent, window, cx| match event {
                        TextInputEvent::Change(_) => {
                            if let Some(size) = this.size.as_mut() {
                                size.preview = None;
                                size.loss = None;
                                size.pending = None;
                            }
                            cx.notify();
                        }
                        TextInputEvent::Submit(_) => this.submit_size(window, cx),
                    },
                )
            })
            .collect();
        Self {
            columns,
            rows,
            preview: None,
            loss: None,
            pending: None,
            _subscriptions: subscriptions,
        }
    }

    fn dimensions(&self, cx: &App) -> Result<(usize, usize), &'static str> {
        let parse = |input: &Entity<TextInput>| {
            let text = input.read(cx).text();
            let text = text.trim();
            (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| text.parse::<usize>().ok())
                .flatten()
        };
        let (Some(columns), Some(rows)) = (parse(&self.columns), parse(&self.rows)) else {
            return Err("请输入有效的整数列数和行数");
        };
        if columns == 0 || rows == 0 {
            return Err("至少保留 1 列和 1 行表头");
        }
        if columns > crate::tables::MAX_COLUMNS {
            return Err("列数最多为 128");
        }
        if rows > crate::tables::MAX_ROWS {
            return Err("行数最多为 2048");
        }
        if !crate::tables::valid_size(columns, rows) {
            return Err("单元格总数不能超过 65536");
        }
        Ok((columns, rows))
    }
}

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("tab", NextField, Some("TinyMdTableSize")),
        gpui::KeyBinding::new("shift-tab", PreviousField, Some("TinyMdTableSize")),
        gpui::KeyBinding::new("enter", ApplySize, Some("TinyMdTableSize")),
        gpui::KeyBinding::new("escape", CancelSize, Some("TinyMdTableSize")),
    ]);
}

impl TableMenu {
    fn submit_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let confirmed = self
            .size
            .as_ref()
            .is_some_and(|size| size.pending.is_some());
        self.apply_size(confirmed, window, cx);
    }

    fn next_size_field(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(size) = &self.size else {
            return;
        };
        let columns = size.columns.read(cx).focus_handle();
        let rows = size.rows.read(cx).focus_handle();
        let input = if columns.is_focused(window) || (self.focus.is_focused(window) && reverse) {
            &size.rows
        } else if rows.is_focused(window) || !reverse {
            &size.columns
        } else {
            return;
        };
        window.focus(&input.read(cx).focus_handle());
        input.update(cx, |input, cx| input.select_all(cx));
    }

    fn apply_size(&mut self, confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(size) = &self.size else {
            return;
        };
        let Ok((columns, rows)) = size.dimensions(cx) else {
            return;
        };
        let confirmed = confirmed && size.pending == Some((columns, rows));
        let Some(editor) = self.editor.upgrade() else {
            self.close(window, cx);
            return;
        };
        let editor = editor.read(cx);
        if editor.read_only || editor.source_mode || !self.target.matches(&editor.model) {
            self.close(window, cx);
            return;
        }
        let Some(loss) = crate::tables::resize_loss(&editor.model, columns, rows) else {
            self.close(window, cx);
            return;
        };
        if loss > 0 && !confirmed {
            self.size.as_mut().unwrap().loss = Some(loss);
            self.size.as_mut().unwrap().pending = Some((columns, rows));
            cx.notify();
            return;
        }
        self.close(window, cx);
        let _ = self.editor.update(cx, |editor, cx| {
            editor.command(
                crate::EditorCommand::TableEdit(TableCommand::Resize { columns, rows }),
                cx,
            );
        });
    }

    fn pick_size(
        &mut self,
        columns: usize,
        rows: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(size) = self.size.as_mut() else {
            return;
        };
        size.columns
            .update(cx, |input, cx| input.set_text(&columns.to_string(), cx));
        size.rows
            .update(cx, |input, cx| input.set_text(&rows.to_string(), cx));
        size.preview = None;
        size.loss = None;
        size.pending = None;
        self.apply_size(false, window, cx);
    }

    pub(super) fn render_size(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let size = self.size.as_ref().unwrap();
        let dimensions = size.dimensions(cx);
        let highlight = size.preview.or(dimensions.ok()).unwrap_or((0, 0));
        let loss = size.loss;
        let t = theme(cx);
        let surface = t.surface().hsla();
        let hover = t.surface_hover().hsla();
        let border = t.border().hsla();
        let text = t.text().hsla();
        let muted = t.dimmed().hsla();
        let primary = t.primary().hsla();
        let selected = t.primary().alpha(0.18);
        let danger = t
            .color(
                guise::theme::ColorName::Red,
                if t.scheme.is_dark() { 4 } else { 7 },
            )
            .hsla();
        let viewport = window.viewport_size();
        let width = 230.0_f32.min((f32::from(viewport.width) - 16.0).max(1.0));
        let height = 414.0_f32.min((f32::from(viewport.height) - 16.0).max(1.0));
        let x = f32::from(self.position.x)
            .max(8.0)
            .min((f32::from(viewport.width) - width - 8.0).max(8.0));
        let y = f32::from(self.position.y)
            .max(8.0)
            .min((f32::from(viewport.height) - height - 8.0).max(8.0));
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .h(px(28.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(label)
        };
        let mut grid = div()
            .id("table-size-grid")
            .flex()
            .flex_col()
            .gap(px(3.0))
            .items_center()
            .py(px(4.0))
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if !hovered {
                    this.size.as_mut().unwrap().preview = None;
                    cx.notify();
                }
            }));
        for row in 1..=10 {
            let mut line = div().flex().gap(px(3.0));
            for column in 1..=6 {
                let id = format!("table-size-cell-{column}-{row}");
                line = line.child(
                    div()
                        .id(SharedString::from(id.clone()))
                        .debug_selector(move || id.clone())
                        .size(px(17.0))
                        .border_1()
                        .border_color(border)
                        .rounded(px(2.0))
                        .cursor_pointer()
                        .when(column <= highlight.0 && row <= highlight.1, |el| {
                            el.bg(selected).border_color(primary)
                        })
                        .on_mouse_move(cx.listener(move |this, _, _, cx| {
                            let size = this.size.as_mut().unwrap();
                            if size.preview != Some((column, row)) {
                                size.preview = Some((column, row));
                                cx.notify();
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.pick_size(column, row, window, cx)
                        })),
                );
            }
            grid = grid.child(line);
        }
        let fields = div()
            .flex()
            .gap(px(8.0))
            .child(
                div()
                    .id("table-size-columns")
                    .debug_selector(|| "table-size-columns".into())
                    .flex_1()
                    .child(div().text_color(muted).child("列"))
                    .child(size.columns.clone()),
            )
            .child(
                div()
                    .id("table-size-rows")
                    .debug_selector(|| "table-size-rows".into())
                    .flex_1()
                    .child(div().text_color(muted).child("行（含表头）"))
                    .child(size.rows.clone()),
            );
        let message = if let Err(message) = dimensions {
            message.to_string()
        } else if let Some(loss) = loss {
            format!("将移除 {loss} 个有内容的单元格")
        } else {
            "从右侧和底部调整，可一次撤销".into()
        };
        let content = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .flex_shrink_0()
            .child(
                div().flex().justify_between().child("表格尺寸").child(
                    div()
                        .text_color(muted)
                        .child(format!("{} × {}", highlight.0, highlight.1)),
                ),
            )
            .child(grid)
            .child(div().h(px(1.0)).bg(border))
            .child(fields)
            .child(
                div()
                    .min_h(px(36.0))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(if dimensions.is_err() || loss.is_some() {
                        danger
                    } else {
                        muted
                    })
                    .child(message),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(4.0))
                    .child(
                        button("table-size-cancel", "取消")
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    )
                    .child(
                        button(
                            "table-size-apply",
                            if loss.is_some() {
                                "确认缩小"
                            } else {
                                "应用"
                            },
                        )
                        .text_color(if dimensions.is_err() {
                            muted
                        } else if loss.is_some() {
                            danger
                        } else {
                            primary
                        })
                        .when(dimensions.is_ok(), |el| {
                            el.on_click(
                                cx.listener(move |this, _, window, cx| {
                                    this.submit_size(window, cx)
                                }),
                            )
                        }),
                    ),
            );
        let panel = div()
            .id("table-size-panel")
            .debug_selector(|| "table-size-panel".into())
            .occlude()
            .track_focus(&self.focus)
            .key_context("TinyMdTableSize")
            .on_action(cx.listener(|this, _: &NextField, window, cx| {
                this.next_size_field(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PreviousField, window, cx| {
                this.next_size_field(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ApplySize, window, cx| this.submit_size(window, cx)))
            .on_action(cx.listener(|this, _: &CancelSize, window, cx| this.close(window, cx)))
            .on_key_down(cx.listener(|this, _, window, cx| {
                // Text input needs the key to reach GPUI's platform handler.
                // The editor ignores keys while this popup owns focus.
                if this.focus.is_focused(window) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .h(px(height))
            .p(px(12.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(surface)
            .text_color(text)
            .text_size(px(12.0))
            .line_height(px(20.0))
            .shadow_md()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(content);
        let backdrop = div()
            .id("table-size-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .w(viewport.width)
            .h(viewport.height)
            .cursor_default()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close(window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    this.close(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(panel);
        div()
            .child(deferred(
                anchored().position(point(px(0.0), px(0.0))).child(backdrop),
            ))
            .into_any_element()
    }
}
