//! Native text input for the adapted Guise view.
use super::*;
use gpui::{EntityInputHandler, UTF16Selection, size};
use std::ops::Range;

impl EntityInputHandler for MarkdownEditor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let source = self.model.text();
        let bytes = ime::byte_range(&source, range);
        *adjusted = Some(
            source[..bytes.start].encode_utf16().count()
                ..source[..bytes.end].encode_utf16().count(),
        );
        Some(source[bytes].to_owned())
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.read_only && !ignore_disabled {
            return None;
        }
        let range = ime::selection(&self.model);
        let reversed = self
            .model
            .selection()
            .is_some_and(|(start, _)| self.model.cursor() == start);
        Some(UTF16Selection { range, reversed })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.ime.marked()
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.finish_composition(cx);
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let text = self.table_input(text, range.clone());
        self.ime.commit(&mut self.model, range, &text);
        self.after_edit(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        self.ime.update(&mut self.model, range, text, selected);
        self.caret.reset();
        self.scroll_to_cursor = true;
        cx.notify(); // Provisional input must not emit a document Change.
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let pos = ime::pos_utf16(&self.model, range.start);
        let row = self.layout.get(pos.line)?;
        let source = self.model.line(pos.line)?;
        let vis = vis_for_src(&row.plan.segs, byte_for_col(source, pos.col));
        let (x, visual_row) = row.caret(vis);
        Some(Bounds::new(
            point(
                self.text_bounds.origin.x + px(row.inset + x),
                self.text_bounds.origin.y
                    + px(row.y + row.pad_top + visual_row as f32 * row.line_h),
            ),
            size(px(1.0), px(row.line_h)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let (line, col) = self.hit(
            f32::from(point.x - self.text_bounds.origin.x),
            f32::from(point.y - self.text_bounds.origin.y),
        );
        Some(ime::offset_utf16(&self.model, Pos::new(line, col)))
    }
}
