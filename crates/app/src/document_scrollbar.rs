use gpui::{Context, IntoElement, Render, ScrollHandle, Window};
use gpui_component::scroll::{Scrollbar, ScrollbarShow};

pub(super) struct DocumentScrollbar {
    pub scroll: ScrollHandle,
}

impl Render for DocumentScrollbar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Always keep a length/position hint when content overflows. The
        // component draws nothing for a document that fits in the viewport.
        Scrollbar::vertical(&self.scroll).scrollbar_show(ScrollbarShow::Always)
    }
}
