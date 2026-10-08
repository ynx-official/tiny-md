//! Read-only diagram windows own their pixels independently of the document.
use super::menus::CloseDocument;
use gpui::{prelude::*, *};
use gpui_component::{
    Root, Sizable, TitleBar,
    button::{Button, ButtonVariants},
};
use tiny_md_editor::{Diagram, DiagramImage};

struct Viewport {
    image: (f32, f32),
    size: (f32, f32),
    zoom: Option<f32>,
    pan: (f32, f32),
}

impl Viewport {
    fn new(width: f32, height: f32) -> Self {
        Self {
            image: (width, height),
            size: (1000.0, 700.0),
            zoom: None,
            pan: (0.0, 0.0),
        }
    }

    fn scale(&self) -> f32 {
        self.zoom.unwrap_or_else(|| {
            ((self.size.0 - 48.0).max(1.0) / self.image.0)
                .min((self.size.1 - 48.0).max(1.0) / self.image.1)
                .min(2.0)
        })
    }

    fn image_size(&self) -> (f32, f32) {
        (self.image.0 * self.scale(), self.image.1 * self.scale())
    }

    fn origin(&self) -> (f32, f32) {
        let (width, height) = self.image_size();
        (
            (self.size.0 - width) / 2.0 + self.pan.0,
            (self.size.1 - height) / 2.0 + self.pan.1,
        )
    }

    fn zoom_at(&mut self, scale: f32, anchor: (f32, f32)) {
        let old_scale = self.scale();
        let origin = self.origin();
        let scale = scale.clamp(0.05, 8.0);
        self.zoom = Some(scale);
        let (width, height) = self.image_size();
        // Keep the same graph point under the pointer when zooming.
        self.pan = (
            anchor.0 - (anchor.0 - origin.0) * scale / old_scale - (self.size.0 - width) / 2.0,
            anchor.1 - (anchor.1 - origin.1) * scale / old_scale - (self.size.1 - height) / 2.0,
        );
    }

    fn fit(&mut self) {
        self.zoom = None;
        self.pan = (0.0, 0.0);
    }
}

struct DiagramViewer {
    viewport: Viewport,
    image: Entity<DiagramImage>,
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
    drag: Option<Point<Pixels>>,
    dark: bool,
}

impl DiagramViewer {
    fn new(diagram: Diagram, dark: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let image = cx.new(|cx| DiagramImage::high_definition(diagram.clone(), cx));
        let close_image = image.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = close_image.update(cx, |image, cx| image.unload(window, cx));
            true
        });
        let focus = cx.focus_handle();
        window.focus(&focus);
        window.set_window_title("流程图查看 — Tiny MD");
        Self {
            viewport: Viewport::new(diagram.width, diagram.height),
            image,
            focus,
            bounds: Bounds::default(),
            drag: None,
            dark,
        }
    }

    fn zoom(&mut self, factor: f32, cx: &mut Context<Self>) {
        self.viewport.zoom_at(
            self.viewport.scale() * factor,
            (self.viewport.size.0 / 2.0, self.viewport.size.1 / 2.0),
        );
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.image.update(cx, |image, cx| image.unload(window, cx));
        window.remove_window();
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        if !cfg!(target_os = "windows") {
            return TitleBar::new()
                .child(div().text_size(px(13.0)).child("流程图查看"))
                .into_any_element();
        }
        let surface = if self.dark {
            rgb(0x252c28)
        } else {
            rgb(0xf0f3f9)
        };
        div()
            .h(px(34.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(surface)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .window_control_area(WindowControlArea::Drag)
                    .text_size(px(13.0))
                    .child("流程图查看"),
            )
            .child(
                Button::new("viewer-minimize")
                    .ghost()
                    .label("−")
                    .w(px(40.0))
                    .h_full()
                    .on_click(|_, window, _| window.minimize_window()),
            )
            .child(
                Button::new("viewer-maximize")
                    .ghost()
                    .label("□")
                    .w(px(40.0))
                    .h_full()
                    .on_click(|_, window, _| window.zoom_window()),
            )
            .child(
                div()
                    .h_full()
                    .debug_selector(|| "viewer-close".into())
                    .child(
                        Button::new("viewer-close")
                            .ghost()
                            .label("×")
                            .w(px(40.0))
                            .h_full()
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    ),
            )
            .into_any_element()
    }
}

impl Render for DiagramViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (x, y) = self.viewport.origin();
        self.image.update(cx, |image, cx| {
            image.set_viewport(
                self.viewport.size,
                (x, y),
                self.viewport.scale(),
                window.scale_factor(),
                cx,
            )
        });
        let entity = cx.entity().downgrade();
        let previous_bounds = self.bounds;
        let canvas_bg = if self.dark {
            rgb(0x1e2421)
        } else {
            rgb(0xffffff)
        };
        let surface = if self.dark {
            rgb(0x252c28)
        } else {
            rgb(0xf6f5f4)
        };
        let border = if self.dark {
            rgb(0x3a443e)
        } else {
            rgb(0xe5e3df)
        };
        let ink = if self.dark {
            rgb(0xe6f1ea)
        } else {
            rgb(0x37352f)
        };
        let title_bar = self.title_bar(cx);
        div()
            .id("diagram-viewer")
            .size_full()
            .flex()
            .flex_col()
            .bg(canvas_bg)
            .text_color(ink)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseDocument, window, cx| {
                cx.stop_propagation();
                this.close(window, cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.close(window, cx);
                }
            }))
            .child(title_bar)
            .child(
                div()
                    .h(px(44.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .bg(surface)
                    .border_b_1()
                    .border_color(border)
                    .child(
                        Button::new("viewer-zoom-out")
                            .small()
                            .ghost()
                            .label("缩小")
                            .on_click(cx.listener(|this, _, _, cx| this.zoom(1.0 / 1.25, cx))),
                    )
                    .child(
                        Button::new("viewer-zoom-in")
                            .small()
                            .ghost()
                            .label("放大")
                            .on_click(cx.listener(|this, _, _, cx| this.zoom(1.25, cx))),
                    )
                    .child(
                        Button::new("viewer-actual")
                            .small()
                            .ghost()
                            .label("100%")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.viewport.zoom_at(
                                    1.0,
                                    (this.viewport.size.0 / 2.0, this.viewport.size.1 / 2.0),
                                );
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("viewer-fit")
                            .small()
                            .ghost()
                            .label("适合窗口")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.viewport.fit();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .child(format!("{:.0}%", self.viewport.scale() * 100.0)),
                    ),
            )
            .child(
                div()
                    .id("diagram-viewer-canvas")
                    .debug_selector(|| "diagram-viewer-canvas".into())
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .cursor(if self.drag.is_some() {
                        CursorStyle::ClosedHand
                    } else {
                        CursorStyle::OpenHand
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.drag = Some(event.position);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.drag = None;
                            return;
                        }
                        if let Some(previous) = this.drag {
                            this.drag = Some(event.position);
                            this.viewport.pan.0 += f32::from(event.position.x - previous.x);
                            this.viewport.pan.1 += f32::from(event.position.y - previous.y);
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.drag = None;
                            cx.notify();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.drag = None;
                            cx.notify();
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                        let delta = f32::from(event.delta.pixel_delta(px(20.0)).y);
                        let position = event.position - this.bounds.origin;
                        this.viewport.zoom_at(
                            this.viewport.scale() * (delta * 0.005).clamp(-0.4, 0.4).exp(),
                            (f32::from(position.x), f32::from(position.y)),
                        );
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(div().absolute().size_full().child(self.image.clone()))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                if bounds == previous_bounds {
                                    return;
                                }
                                let entity = entity.clone();
                                window.on_next_frame(move |_, cx| {
                                    let _ = entity.update(cx, |this, cx| {
                                        let size = (
                                            f32::from(bounds.size.width),
                                            f32::from(bounds.size.height),
                                        );
                                        this.bounds = bounds;
                                        if this.viewport.size != size {
                                            this.viewport.size = size;
                                            cx.notify();
                                        }
                                    });
                                });
                            },
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .bg(surface)
                    .border_t_1()
                    .border_color(border)
                    .text_size(px(11.0))
                    .child("滚轮缩放 · 左键拖动 · Esc 关闭"),
            )
    }
}

pub(super) fn open(diagram: Diagram, dark: bool, cx: &mut App) -> gpui::Result<()> {
    let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Maximized(bounds)),
            window_min_size: Some(size(px(600.0), px(400.0))),
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        },
        |window, cx| {
            let viewer = cx.new(|cx| DiagramViewer::new(diagram, dark, window, cx));
            cx.new(|cx| Root::new(viewer, window, cx))
        },
    )?;
    cx.activate(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Viewport;
    use super::{CloseDocument, DiagramViewer};
    use gpui::{
        Image, ImageFormat, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TestAppContext,
        point, px,
    };
    use std::sync::Arc;
    use tiny_md_editor::Diagram;

    #[gpui::test]
    fn native_close_button_accepts_closing_the_viewer(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let diagram = Diagram {
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, br##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" viewBox="0 0 400 200"><rect width="400" height="200" fill="#edf6f1"/></svg>"##.to_vec())),
            width: 400.0, height: 200.0,
        };
        let (view, cx) =
            cx.add_window_view(|window, cx| DiagramViewer::new(diagram, false, window, cx));
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear();
            });
        }
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(101));
        cx.run_until_parked();
        assert!(cx.read(|cx| view.read(cx).image.read(cx).decoded_size().is_some()));
        assert!(
            cx.simulate_close(),
            "the native close button must be accepted independently of keyboard actions"
        );
        assert!(
            cx.read(|cx| view.read(cx).image.read(cx).decoded_size().is_none()),
            "native close must unload the high-definition bitmap immediately"
        );
    }

    #[cfg(target_os = "windows")]
    #[gpui::test]
    fn clicking_the_titlebar_close_button_unloads_only_the_viewer(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let original = cx.add_window(|_, _| gpui::Empty).into();
        let diagram = Diagram {
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, br##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" viewBox="0 0 400 200"><rect width="400" height="200" fill="#edf6f1"/></svg>"##.to_vec())),
            width: 400.0, height: 200.0,
        };
        let (view, cx) =
            cx.add_window_view(|window, cx| DiagramViewer::new(diagram, false, window, cx));
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear();
            });
        }
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(101));
        cx.run_until_parked();
        assert!(cx.read(|cx| view.read(cx).image.read(cx).decoded_size().is_some()));
        let close = cx
            .debug_bounds("viewer-close")
            .expect("Windows titlebar close button");
        cx.simulate_click(close.center(), Modifiers::none());
        cx.read(|cx| {
            assert!(cx.windows() == vec![original]);
            assert!(view.read(cx).image.read(cx).decoded_size().is_none());
        });
    }

    #[gpui::test]
    fn viewer_mouse_zoom_drag_and_close_only_affect_the_viewer(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let diagram = Diagram {
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, br##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="2400" viewBox="0 0 400 2400"><rect width="400" height="2400" fill="#edf6f1"/></svg>"##.to_vec())),
            width: 400.0, height: 2400.0,
        };
        let original = cx.add_window(|_, _| gpui::Empty).into();
        let (view, cx) =
            cx.add_window_view(|window, cx| DiagramViewer::new(diagram, false, window, cx));
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear();
            });
        }
        let bounds = cx
            .debug_bounds("diagram-viewer-canvas")
            .expect("visible viewer canvas");
        assert!(bounds.size.width > px(0.0) && bounds.size.height > px(0.0));
        let anchor = bounds.center();
        let before = cx.read(|cx| view.read(cx).viewport.scale());
        cx.simulate_event(ScrollWheelEvent {
            position: anchor,
            delta: ScrollDelta::Pixels(point(px(0.0), px(80.0))),
            ..Default::default()
        });
        assert!(cx.read(|cx| view.read(cx).viewport.scale()) > before);
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        let before = cx.read(|cx| view.read(cx).viewport.pan);
        cx.simulate_mouse_down(anchor, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(
            anchor + point(px(40.0), px(60.0)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            anchor + point(px(40.0), px(60.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        let after = cx.read(|cx| view.read(cx).viewport.pan);
        assert!((after.0 - before.0 - 40.0).abs() < 0.01);
        assert!((after.1 - before.1 - 60.0).abs() < 0.01);
        cx.dispatch_action(CloseDocument);
        cx.read(|cx| {
            assert!(
                cx.windows() == vec![original],
                "closing the viewer must leave the original window open"
            )
        });
    }

    #[test]
    fn fit_shows_both_axes_and_reset_clears_drag_and_zoom() {
        let mut view = Viewport::new(400.0, 2400.0);
        assert!(view.image_size().0 <= 952.0);
        assert!(view.image_size().1 <= 652.0);
        view.pan = (100.0, -200.0);
        view.zoom_at(2.0, (500.0, 350.0));
        view.fit();
        assert_eq!(view.zoom, None);
        assert_eq!(view.pan, (0.0, 0.0));
    }

    #[test]
    fn zoom_keeps_the_graph_point_under_the_mouse_and_has_limits() {
        let mut view = Viewport::new(400.0, 2400.0);
        let anchor = (137.0, 291.0);
        let origin = view.origin();
        let graph_point = (
            (anchor.0 - origin.0) / view.scale(),
            (anchor.1 - origin.1) / view.scale(),
        );
        view.zoom_at(2.0, anchor);
        let origin = view.origin();
        assert!((origin.0 + graph_point.0 * view.scale() - anchor.0).abs() < 0.01);
        assert!((origin.1 + graph_point.1 * view.scale() - anchor.1).abs() < 0.01);
        view.zoom_at(100.0, anchor);
        assert_eq!(view.scale(), 8.0);
        view.zoom_at(0.0, anchor);
        assert_eq!(view.scale(), 0.05);
    }
}
