use gpui::{Context, IntoElement, Pixels, Point, Render, ScrollHandle, Size, Window};
use gpui_component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarShow};
use std::{cell::Cell, rc::Rc};

pub(super) struct DocumentScrollbar {
    pub scroll: ScrollHandle,
    pub dragging: Rc<Cell<bool>>,
}

#[derive(Clone)]
struct DocumentScrollHandle {
    scroll: ScrollHandle,
    dragging: Rc<Cell<bool>>,
}

impl ScrollbarHandle for DocumentScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.scroll.offset()
    }
    fn set_offset(&self, offset: Point<Pixels>) {
        self.scroll.set_offset(offset);
    }
    fn content_size(&self) -> Size<Pixels> {
        self.scroll.max_offset() + self.scroll.bounds().size
    }
    fn start_drag(&self) {
        self.dragging.set(true);
    }
    fn end_drag(&self) {
        self.dragging.set(false);
    }
}

impl Render for DocumentScrollbar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Always keep a length/position hint when content overflows. The
        // component draws nothing for a document that fits in the viewport.
        Scrollbar::vertical(&DocumentScrollHandle {
            scroll: self.scroll.clone(),
            dragging: self.dragging.clone(),
        })
        .scrollbar_show(ScrollbarShow::Always)
    }
}
