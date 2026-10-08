//! The cached document and blinking caret have independent invalidation.
use super::MarkdownEditor;
use gpui::{
    Bounds, Context, Entity, Hsla, Pixels, Render, Subscription, WeakEntity, Window, canvas, fill,
    prelude::*,
};

pub(super) struct DocumentLayer {
    editor: WeakEntity<MarkdownEditor>,
    _editor_changes: Subscription,
}

impl DocumentLayer {
    pub fn new(editor: Entity<MarkdownEditor>, cx: &mut Context<Self>) -> Self {
        let editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        Self {
            editor: editor.downgrade(),
            _editor_changes: editor_changes,
        }
    }
}

impl Render for DocumentLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.editor
            .update(cx, |editor, cx| editor.render_document(window, cx))
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}

pub(super) struct CaretLayer {
    pub visible: bool,
    pub bounds: Option<Bounds<Pixels>>,
    pub color: Hsla,
}

impl Default for CaretLayer {
    fn default() -> Self {
        Self {
            visible: false,
            bounds: None,
            color: gpui::black(),
        }
    }
}

impl Render for CaretLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let caret = self.bounds.filter(|_| self.visible);
        let color = self.color;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                if let Some(caret) = caret {
                    window.paint_quad(fill(
                        Bounds::new(bounds.origin + caret.origin, caret.size),
                        color,
                    ));
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}
