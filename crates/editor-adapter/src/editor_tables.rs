//! Markdown table controls, shared by Windows and macOS.
//! Popup positioning follows Guise's MIT context menu (see THIRD_PARTY.md).
use super::*;
use crate::tables::{TableCommand, TableContext};
use gpui::{Point, WeakEntity, anchored, deferred};

pub(super) const TOOLBAR_HEIGHT: f32 = 26.0;

#[derive(Clone)]
struct Target {
    cell: TableContext,
    cursor: Pos,
    selection: Option<(Pos, Pos)>,
    source: String,
}

impl Target {
    fn capture(model: &EditorModel, cell: TableContext) -> Self {
        Self {
            cell,
            cursor: model.cursor(),
            selection: model.selection(),
            source: model.lines()[cell.start..=cell.end].join("\n"),
        }
    }

    fn matches(&self, model: &EditorModel) -> bool {
        self.cursor == model.cursor()
            && self.selection == model.selection()
            && model
                .lines()
                .get(self.cell.start..=self.cell.end)
                .is_some_and(|lines| lines.join("\n") == self.source)
    }
}

pub(super) fn inside_html_table(doc: &crate::html::Document, start: usize, end: usize) -> bool {
    doc.tags.iter().any(|tag| {
        tag.name == "table"
            && !tag.close
            && doc.line(tag.range.start) <= end
            && tag
                .mate
                .is_none_or(|mate| doc.line(doc.tags[mate].range.end.saturating_sub(1)) >= start)
    })
}

#[derive(Clone, Copy)]
enum Operation {
    Edit(TableCommand),
    Copy,
}

struct Item {
    id: &'static str,
    label: &'static str,
    operation: Operation,
    enabled: bool,
    separator: bool,
}

fn items(cell: TableContext) -> Vec<Item> {
    use TableCommand::*;
    [
        (
            "table-row-above",
            "上方插入行",
            Operation::Edit(RowAbove),
            false,
        ),
        (
            "table-row-below",
            "下方插入行",
            Operation::Edit(RowBelow),
            false,
        ),
        (
            "table-column-left",
            "左侧插入列",
            Operation::Edit(ColumnLeft),
            false,
        ),
        (
            "table-column-right",
            "右侧插入列",
            Operation::Edit(ColumnRight),
            false,
        ),
        (
            "table-move-row-up",
            "上移该行",
            Operation::Edit(MoveRowUp),
            true,
        ),
        (
            "table-move-row-down",
            "下移该行",
            Operation::Edit(MoveRowDown),
            false,
        ),
        (
            "table-move-column-left",
            "左移该列",
            Operation::Edit(MoveColumnLeft),
            false,
        ),
        (
            "table-move-column-right",
            "右移该列",
            Operation::Edit(MoveColumnRight),
            false,
        ),
        (
            "table-delete-row",
            "删除行",
            Operation::Edit(DeleteRow),
            true,
        ),
        (
            "table-delete-column",
            "删除列",
            Operation::Edit(DeleteColumn),
            false,
        ),
        ("table-copy", "复制表格", Operation::Copy, true),
        (
            "table-format",
            "格式化表格源码",
            Operation::Edit(Format),
            false,
        ),
        (
            "table-delete",
            "删除表格",
            Operation::Edit(DeleteTable),
            true,
        ),
    ]
    .into_iter()
    .map(|(id, label, operation, separator)| Item {
        id,
        label,
        operation,
        separator,
        enabled: match operation {
            Operation::Edit(command) => cell.allows(command),
            Operation::Copy => true,
        },
    })
    .collect()
}

gpui::actions!(
    tiny_md_table_menu,
    [
        NextItem,
        PreviousItem,
        FirstItem,
        LastItem,
        Confirm,
        Dismiss
    ]
);

pub(super) struct TableMenu {
    editor: WeakEntity<MarkdownEditor>,
    target: Target,
    items: Vec<Item>,
    focus: FocusHandle,
    return_focus: FocusHandle,
    position: Point<Pixels>,
    scroll: ScrollHandle,
    selected: usize,
    open: bool,
}

impl TableMenu {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn valid(&self, model: &EditorModel) -> bool {
        self.target.matches(model)
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if self.focus.is_focused(window) {
            window.focus(&self.return_focus);
        }
        cx.notify();
    }

    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.items[index].enabled {
            return;
        }
        let operation = self.items[index].operation;
        self.close(window, cx);
        let _ = self.editor.update(cx, |editor, cx| {
            if editor.read_only || editor.source_mode || !self.target.matches(&editor.model) {
                return;
            }
            match operation {
                Operation::Copy => {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.target.source.clone()))
                }
                Operation::Edit(command) => {
                    editor.command(crate::EditorCommand::TableEdit(command), cx)
                }
            }
            cx.notify();
        });
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        for n in 1..=self.items.len() {
            let index = if forward {
                (self.selected + n) % self.items.len()
            } else {
                (self.selected + self.items.len() - n) % self.items.len()
            };
            if self.items[index].enabled {
                self.selected = index;
                self.scroll.scroll_to_item(index);
                cx.notify();
                break;
            }
        }
    }
}

impl Render for TableMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let t = theme(cx);
        let surface = t.surface().hsla();
        let hover = t.surface_hover().hsla();
        let border = t.border().hsla();
        let text = t.text().hsla();
        let muted = t.dimmed().hsla();
        let danger = t
            .color(
                guise::theme::ColorName::Red,
                if t.scheme.is_dark() { 4 } else { 7 },
            )
            .hsla();
        let viewport = window.viewport_size();
        let width = 210.0_f32.min((f32::from(viewport.width) - 16.0).max(1.0));
        let height = (self.items.len() as f32 * 28.0 + 4.0 * 9.0 + 8.0)
            .min((f32::from(viewport.height) - 16.0).max(1.0));
        let x = (f32::from(self.position.x) - width)
            .max(8.0)
            .min((f32::from(viewport.width) - width - 8.0).max(8.0));
        let y = f32::from(self.position.y)
            .max(8.0)
            .min((f32::from(viewport.height) - height - 8.0).max(8.0));
        let mut menu = div()
            .id("table-menu")
            .debug_selector(|| "table-menu".into())
            .occlude()
            .track_focus(&self.focus)
            .key_context("TinyMdTableMenu")
            .on_action(cx.listener(|this, _: &NextItem, _, cx| this.step(true, cx)))
            .on_action(cx.listener(|this, _: &PreviousItem, _, cx| this.step(false, cx)))
            .on_action(cx.listener(|this, _: &FirstItem, _, cx| {
                this.selected = this.items.len() - 1;
                this.step(true, cx);
            }))
            .on_action(cx.listener(|this, _: &LastItem, _, cx| {
                this.selected = 0;
                this.step(false, cx);
            }))
            .on_action(
                cx.listener(|this, _: &Confirm, window, cx| this.choose(this.selected, window, cx)),
            )
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| this.close(window, cx)))
            .on_key_down(|_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(|_, _, cx| cx.stop_propagation())
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(width))
            .h(px(height))
            .flex()
            .flex_col()
            .p(px(4.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(border)
            .bg(surface)
            .text_size(px(13.0))
            .line_height(px(20.0))
            .shadow_md()
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        for (index, item) in self.items.iter().enumerate() {
            let id = item.id;
            let enabled = item.enabled;
            let mut row = div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_shrink_0();
            if item.separator {
                row = row.child(div().my(px(4.0)).h(px(1.0)).bg(border));
            }
            row = row.child(
                div()
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .text_color(if !enabled {
                        muted
                    } else if matches!(item.operation, Operation::Edit(TableCommand::DeleteTable)) {
                        danger
                    } else {
                        text
                    })
                    .when(enabled && self.selected == index, |el| el.bg(hover))
                    .child(item.label),
            );
            if enabled {
                row = row
                    .cursor_pointer()
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.selected != index {
                            this.selected = index;
                            cx.notify();
                        }
                    }))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.choose(index, window, cx)),
                    );
            }
            menu = menu.child(row);
        }
        let backdrop = div()
            .id("table-menu-backdrop")
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
            .child(menu);
        div()
            .child(deferred(
                anchored().position(point(px(0.0), px(0.0))).child(backdrop),
            ))
            .into_any_element()
    }
}

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("down", NextItem, Some("TinyMdTableMenu")),
        gpui::KeyBinding::new("tab", NextItem, Some("TinyMdTableMenu")),
        gpui::KeyBinding::new("up", PreviousItem, Some("TinyMdTableMenu")),
        gpui::KeyBinding::new("shift-tab", PreviousItem, Some("TinyMdTableMenu")),
        gpui::KeyBinding::new("home", FirstItem, Some("TinyMdTableMenu")),
        gpui::KeyBinding::new("end", LastItem, Some("TinyMdTableMenu")),
        gpui::KeyBinding::new("enter", Confirm, Some("TinyMdTableMenu")),
        gpui::KeyBinding::new("escape", Dismiss, Some("TinyMdTableMenu")),
    ]);
}

impl MarkdownEditor {
    pub(super) fn table_context(&self) -> Option<TableContext> {
        // Commands can arrive before the next render. Do not update the live
        // cache here: render needs its old lines to rebase cached row geometry.
        let parsed = self
            .document_cache
            .current()
            .filter(|parsed| parsed.lines == self.model.lines())
            .unwrap_or_else(|| DocumentCache::default().update(self.model.lines()).0);
        let cell = parsed
            .tables
            .iter()
            .find_map(|table| table.context(&self.model))?;
        (!inside_html_table(&parsed.html, cell.start, cell.end)).then_some(cell)
    }

    pub(super) fn table_menu_open(&self, cx: &App) -> bool {
        self.table_menu
            .as_ref()
            .is_some_and(|menu| menu.read(cx).is_open())
    }

    pub(super) fn open_table_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only || self.source_mode {
            return;
        }
        self.finish_composition(cx);
        let Some(cell) = self.table_context() else {
            return;
        };
        let target = Target::capture(&self.model, cell);
        let items = items(target.cell);
        let selected = items.iter().position(|item| item.enabled).unwrap_or(0);
        let return_focus = self.focus.clone();
        let editor = cx.entity().downgrade();
        let menu = cx.new(|cx| TableMenu {
            editor,
            target,
            items,
            selected,
            return_focus,
            position,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            open: true,
        });
        window.focus(&menu.read(cx).focus);
        self.table_menu_changes = Some(cx.observe(&menu, |_, _, cx| cx.notify()));
        self.table_menu = Some(menu);
        cx.notify();
    }

    pub(super) fn table_toolbar(&self, cell: TableContext, cx: &Context<Self>) -> AnyElement {
        let t = theme(cx);
        let text = t.dimmed().hsla();
        let hover = t.surface_hover().hsla();
        let selected = t.primary().alpha(0.12);
        let active = t.primary().hsla();
        let button = |id: &'static str, icon: IconName, label: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_pointer()
                .text_size(px(14.0))
                .text_color(text)
                .hover(move |style| style.bg(hover))
                .tooltip(guise::overlay::tooltip(label))
                .child(Glyph::Lucide(icon))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
        };
        let mut left = div().flex().items_center().gap(px(2.0));
        for (id, icon, label, alignment) in [
            (
                "table-align-left",
                IconName::AlignLeft,
                "当前列左对齐",
                Alignment::Left,
            ),
            (
                "table-align-center",
                IconName::AlignCenter,
                "当前列居中",
                Alignment::Center,
            ),
            (
                "table-align-right",
                IconName::AlignRight,
                "当前列右对齐",
                Alignment::Right,
            ),
        ] {
            left = left.child(
                button(id, icon, label)
                    .when(cell.alignment == alignment, |el| {
                        el.bg(selected).text_color(active)
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus);
                        this.command(
                            crate::EditorCommand::TableEdit(TableCommand::Align(alignment)),
                            cx,
                        );
                        cx.stop_propagation();
                    })),
            );
        }
        let right = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(
                button(
                    "table-more",
                    IconName::EllipsisVertical,
                    "更多表格操作 · Shift+F10",
                )
                .on_click(cx.listener(
                    |this, event: &gpui::ClickEvent, window, cx| {
                        this.open_table_menu(event.position(), window, cx);
                        cx.stop_propagation();
                    },
                )),
            )
            .child(
                button(
                    "table-delete-button",
                    IconName::Trash2,
                    "删除整张表格（可撤销）",
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus);
                    this.command(
                        crate::EditorCommand::TableEdit(TableCommand::DeleteTable),
                        cx,
                    );
                    cx.stop_propagation();
                })),
            );
        div()
            .id("table-toolbar")
            .debug_selector(|| "table-toolbar".into())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(TOOLBAR_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .cursor_default()
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(left)
            .child(right)
            .into_any_element()
    }
}
