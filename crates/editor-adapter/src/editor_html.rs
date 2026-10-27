//! Native table layout and disclosure controls. HTML previews never mutate EditorModel.
use super::*;
use crate::html;

pub(super) struct TableLayout {
    pub cells: Vec<CellPreview>,
    pub height: f32,
}

pub(super) struct CellPreview {
    pub row: Row,
    pub position: Pos,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub header: bool,
    pub images: Vec<crate::markdown_images::ImageLink>,
    pub image_heights: Vec<f32>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn shape_table(
    table: &html::Table,
    html: &html::Document,
    lines: &[String],
    width: f32,
    style: &CellStyle,
    images: &[Vec<crate::markdown_images::ImageLink>],
    entries: &std::collections::HashMap<String, editor_images::State>,
    window: &mut Window,
) -> TableLayout {
    let source = lines.join("\n");
    let column = width / table.cols.max(1) as f32;
    let mut heights = vec![style.base * 1.6 + 20.0; table.rows];
    let mut cells = Vec::new();
    for cell in &table.cells {
        let text = &source[cell.content.clone()];
        let align = match cell.align {
            1 => Alignment::Center,
            2 => Alignment::Right,
            _ => Alignment::Left,
        };
        let mut row = shape_planned_cell(
            html.plan(
                text,
                cell.content.start,
                &Block::Paragraph,
                None,
                false,
                &[],
            ),
            cell.header,
            column * cell.colspan as f32,
            align,
            style,
            window,
        );
        let images: Vec<_> = images
            .iter()
            .flatten()
            .filter(|image| cell.content.contains(&image.source_offset))
            .cloned()
            .collect();
        let image_heights: Vec<_> = images
            .iter()
            .map(|link| {
                let height = match entries.get(&link.target) {
                    Some(editor_images::State::Ready(image)) => {
                        crate::markdown_images::display_size_for(image, row.wrap, link).1
                    }
                    _ => 96.0,
                };
                height + crate::markdown_images::GAP * 2.0
            })
            .collect();
        if row.plan.visible.trim().is_empty() && !images.is_empty() {
            row.height = 0.0;
            row.text = None;
        }
        let content_height = row.height + image_heights.iter().sum::<f32>();
        let current: f32 = heights[cell.row..cell.row + cell.rowspan].iter().sum();
        let increase = (content_height + 20.0 - current).max(0.0) / cell.rowspan as f32;
        for h in &mut heights[cell.row..cell.row + cell.rowspan] {
            *h += increase;
        }
        let line = html.line(cell.content.start);
        let position = Pos::new(
            line,
            col_for_byte(&lines[line], cell.content.start - html.offsets[line]),
        );
        cells.push(CellPreview {
            row,
            position,
            x: column * cell.col as f32,
            y: 0.0,
            width: column * cell.colspan as f32,
            height: 0.0,
            header: cell.header,
            images,
            image_heights,
        });
    }
    for (cell, spec) in cells.iter_mut().zip(&table.cells) {
        cell.y = heights[..spec.row].iter().sum();
        cell.height = heights[spec.row..spec.row + spec.rowspan].iter().sum();
    }
    TableLayout {
        cells,
        height: heights.iter().sum(),
    }
}

impl MarkdownEditor {
    pub(super) fn html_table_element(
        &self,
        table: &TableLayout,
        source_line: usize,
        border: Hsla,
        header: Hsla,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut el = div()
            .relative()
            .w_full()
            .h(px(table.height))
            .debug_selector(|| "html-table".into());
        for (index, cell) in table.cells.iter().enumerate() {
            let mut item = div()
                .id((
                    SharedString::from(format!("html-cell-{source_line}")),
                    index,
                ))
                .debug_selector(move || format!("html-table-cell-{index}"))
                .absolute()
                .left(px(cell.x))
                .top(px(cell.y))
                .w(px(cell.width))
                .h(px(cell.height))
                .border_1()
                .border_color(border)
                .overflow_hidden();
            if cell.header {
                item = item.bg(header);
            }
            if !cell.images.is_empty() {
                item = item.child(
                    div()
                        .absolute()
                        .left(px(cell.row.inset))
                        .top(px(10.0))
                        .w(px(cell.row.wrap))
                        .child(self.image_elements(
                            cell.position.line,
                            &cell.images,
                            &cell.image_heights,
                            cell.row.wrap,
                            cell.row.height + cell.image_heights.iter().sum::<f32>(),
                            border,
                            header,
                            cx,
                        )),
                );
            }
            if let Some(shaped) = cell.row.text.clone() {
                let align = cell.row.align;
                let line_h = px(cell.row.line_h);
                item = item.child(
                    div()
                        .absolute()
                        .left(px(cell.row.inset))
                        .top(px(10.0))
                        .w(px(cell.row.wrap))
                        .h(px(cell.row.height))
                        .child(canvas(
                            |_, _, _| (),
                            move |bounds, _, window, cx| {
                                shaped
                                    .paint_background(
                                        bounds.origin,
                                        line_h,
                                        align,
                                        None,
                                        window,
                                        cx,
                                    )
                                    .ok();
                                shaped
                                    .paint(bounds.origin, line_h, align, None, window, cx)
                                    .ok();
                            },
                        )),
                );
            }
            if !self.read_only {
                let position = cell.position;
                item = item.cursor_text().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.finish_composition(cx);
                        this.model.move_to(position.line, position.col, false);
                        this.caret.reset();
                        window.focus(&this.focus);
                        cx.notify();
                    }),
                );
            }
            for (link_index, (bounds, target)) in cell.row.link_regions().into_iter().enumerate() {
                item = item.child(
                    div()
                        .absolute()
                        .left(bounds.origin.x)
                        .top(bounds.origin.y + px(10.0))
                        .w(bounds.size.width)
                        .h(bounds.size.height)
                        .cursor_pointer()
                        .debug_selector(move || format!("html-table-link-{index}-{link_index}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                if event.modifiers.cmd() || this.read_only {
                                    this.finish_composition(cx);
                                    window.focus(&this.focus);
                                    cx.emit(MarkdownEditorEvent::LinkClick(target.clone()));
                                    cx.stop_propagation();
                                }
                            }),
                        ),
                );
            }
            el = el.child(item);
        }
        el
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn details_toggle(
        &self,
        index: usize,
        key: u64,
        summary: Pos,
        open: bool,
        size: f32,
        color: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(("html-details", index))
            .debug_selector(move || format!("html-details-toggle-{index}"))
            .absolute()
            .left_0()
            .top_0()
            .w(px(22.0))
            .h(px(size * 1.6))
            .cursor_pointer()
            .text_color(color)
            .text_size(px(size))
            .child(if open { "▾" } else { "▸" })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if open && !this.read_only {
                        this.finish_composition(cx);
                        this.model.move_to(summary.line, summary.col, false);
                        this.caret.reset();
                    }
                    this.html_details.insert(key, !open);
                    this.diagram_geometry_dirty = true;
                    cx.notify();
                }),
            )
    }
}
