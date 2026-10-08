use crate::*;

const MIN_SIDEBAR_WIDTH: Pixels = px(220.0);
const MIN_EDITOR_WIDTH: Pixels = px(320.0);

pub(crate) struct SidebarSizing {
    preferred_width: Pixels,
    drag: Option<(Pixels, Pixels)>,
}

impl Default for SidebarSizing {
    fn default() -> Self {
        Self {
            preferred_width: px(250.0),
            drag: None,
        }
    }
}

impl SidebarSizing {
    fn constrain(width: Pixels, window_width: Pixels) -> Pixels {
        width.clamp(
            MIN_SIDEBAR_WIDTH,
            (window_width - MIN_EDITOR_WIDTH).max(MIN_SIDEBAR_WIDTH),
        )
    }

    pub(crate) fn width(&self, window_width: Pixels) -> Pixels {
        // Shrinking a window temporarily limits the layout, preserving the user's preferred width.
        Self::constrain(self.preferred_width, window_width)
    }

    pub(crate) fn finish(&mut self) -> bool {
        self.drag.take().is_some()
    }
}

impl TinyMd {
    pub(crate) fn sidebar_resize_handle(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let view = cx.weak_entity();
        let dragging = self.sidebar_sizing.drag.is_some();
        let highlight = rgb(if self.dark { 0x737b86 } else { 0xc8c4be });
        div()
            .id("sidebar-resize-handle")
            .debug_selector(|| "sidebar-resize-handle".into())
            .occlude()
            .absolute()
            .top_0()
            .right_0()
            .w(px(8.0))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(move |style| style.bg(highlight))
            .when(dragging, |handle| handle.bg(highlight))
            .on_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                if event.button == MouseButton::Left {
                    this.sidebar_sizing.drag = Some((
                        event.position.x,
                        this.sidebar_sizing.width(window.viewport_size().width),
                    ));
                    cx.notify();
                }
                // The resize strip owns clicks; a right click must not open a document menu behind it.
                cx.stop_propagation();
            }))
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        if dragging {
                            window.set_window_cursor_style(CursorStyle::ResizeLeftRight);
                        }
                        // Capture at window level: moving over the editor must not start text selection,
                        // and releasing outside the narrow handle must still end the resize.
                        let moving = view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if !phase.capture() {
                                return;
                            }
                            let _ = moving.update(cx, |this, cx| {
                                let Some((start, width)) = this.sidebar_sizing.drag else {
                                    return;
                                };
                                if event.pressed_button != Some(MouseButton::Left)
                                    || !window.is_window_active()
                                {
                                    this.sidebar_sizing.finish();
                                    cx.notify();
                                    return;
                                }
                                this.sidebar_sizing.preferred_width = SidebarSizing::constrain(
                                    width + event.position.x - start,
                                    window.viewport_size().width,
                                );
                                cx.stop_propagation();
                                cx.notify();
                            });
                        });
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase.capture() && event.button == MouseButton::Left {
                                let _ = view.update(cx, |this, cx| {
                                    if this.sidebar_sizing.finish() {
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}
