use super::*;
use core::prelude::v1::test;

fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        guise_theme(false).init(cx);
        cx.set_global(Session::default());
    });
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
}

fn sidebar(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.debug_bounds("sidebar-panel").expect("visible sidebar")
}

fn activate(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
}

fn drag(cx: &mut VisualTestContext, delta: f32) {
    let bounds = sidebar(cx);
    let start = point(bounds.right() - px(3.0), bounds.center().y);
    let end = start + point(px(delta), px(0.0));
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    draw(cx);
}

#[gpui::test]
fn sidebar_drag_resizes_all_modes_without_editing_or_losing_focus(cx: &mut TestAppContext) {
    init(cx);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view.install(
            Document::untitled("# Note\n\nText"),
            "# Note\n\nText",
            window,
            cx,
        );
        view
    });
    activate(cx);
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    draw(cx);
    assert_eq!(sidebar(cx).size.width, px(250.0));
    drag(cx, 87.5);
    assert_eq!(
        sidebar(cx).size.width,
        px(337.5),
        "drag must continuously resize the sidebar"
    );
    for mode in [
        SidebarMode::Tree,
        SidebarMode::Outline,
        SidebarMode::Documents,
    ] {
        cx.update(|_, cx| view.update(cx, |this, cx| this.show_sidebar(mode, cx)));
        draw(cx);
        assert_eq!(
            sidebar(cx).size.width,
            px(337.5),
            "modes share the chosen width"
        );
    }
    drag(cx, -50.0);
    assert_eq!(sidebar(cx).size.width, px(287.5));
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).text(), "# Note\n\nText");
        assert!(!view.dirty);
        assert!(view.editor.read(cx).focus_handle().is_focused(window));
    });
    let after = sidebar(cx).size.width;
    cx.simulate_mouse_move(point(px(700.0), px(180.0)), None, Modifiers::none());
    draw(cx);
    assert_eq!(
        sidebar(cx).size.width,
        after,
        "release ends resizing over the editor"
    );
}

#[gpui::test]
fn sidebar_width_survives_hiding_focus_mode_and_window_resize(cx: &mut TestAppContext) {
    init(cx);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view
    });
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    activate(cx);
    draw(cx);
    drag(cx, 250.0);
    assert_eq!(sidebar(cx).size.width, px(500.0));
    for focus_mode in [false, true] {
        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.sidebar = focus_mode;
                this.focus_mode = focus_mode;
                cx.notify();
            })
        });
        draw(cx);
        assert_eq!(
            cx.debug_bounds("document-editor-pane").unwrap().size.width,
            px(1000.0),
            "hiding the sidebar releases its entire width to the editor"
        );
        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.sidebar = true;
                this.focus_mode = false;
                cx.notify();
            })
        });
        draw(cx);
        assert_eq!(sidebar(cx).size.width, px(500.0));
    }
    cx.simulate_resize(size(px(600.0), px(700.0)));
    draw(cx);
    assert_eq!(
        sidebar(cx).size.width,
        px(280.0),
        "keep 320 px available for the editor"
    );
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    draw(cx);
    assert_eq!(
        sidebar(cx).size.width,
        px(500.0),
        "restore the requested width when space returns"
    );
}

#[gpui::test]
fn sidebar_drag_limits_width_and_ignores_hover_and_other_buttons(cx: &mut TestAppContext) {
    init(cx);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view
    });
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    draw(cx);
    activate(cx);
    draw(cx);
    let bounds = sidebar(cx);
    let start = point(bounds.right() - px(3.0), bounds.center().y);
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_move(
        start + point(px(80.0), px(0.0)),
        Some(MouseButton::Right),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(start, MouseButton::Right, Modifiers::none());
    draw(cx);
    assert_eq!(sidebar(cx).size.width, px(250.0));
    drag(cx, -1000.0);
    assert_eq!(
        sidebar(cx).size.width,
        px(220.0),
        "minimum keeps sidebar controls usable"
    );
    drag(cx, 1000.0);
    assert_eq!(
        sidebar(cx).size.width,
        px(680.0),
        "maximum follows available window space"
    );
}

#[gpui::test]
fn interrupted_sidebar_drag_preserves_unsaved_text_and_undo(cx: &mut TestAppContext) {
    init(cx);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view.install(Document::untitled("Note"), "Note", window, cx);
        view.editor.update(cx, |editor, cx| {
            editor.command(EditorCommand::Wrap("**"), cx)
        });
        view
    });
    activate(cx);
    cx.simulate_resize(size(px(1000.0), px(700.0)));
    draw(cx);
    let text = cx.read(|cx| view.read(cx).editor.read(cx).text());
    assert!(cx.read(|cx| view.read(cx).dirty));
    for interruption in 0..3 {
        let bounds = sidebar(cx);
        let start = point(bounds.right() - px(3.0), bounds.center().y);
        cx.simulate_mouse_move(start, None, Modifiers::none());
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        let end = start + point(px(20.0), px(0.0));
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
        draw(cx);
        let width = sidebar(cx).size.width;
        match interruption {
            0 => cx.simulate_mouse_move(end, None, Modifiers::none()),
            1 => {
                cx.deactivate_window();
                draw(cx);
                activate(cx);
            }
            _ => {
                for visible in [false, true] {
                    cx.update(|_, cx| {
                        view.update(cx, |this, cx| {
                            this.sidebar = visible;
                            cx.notify();
                        })
                    });
                    draw(cx);
                }
            }
        }
        cx.simulate_mouse_move(
            end + point(px(80.0), px(0.0)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        draw(cx);
        assert_eq!(
            sidebar(cx).size.width,
            width,
            "an interrupted drag must not resume on its own"
        );
        cx.read(|cx| {
            assert_eq!(view.read(cx).editor.read(cx).text(), text);
            assert!(view.read(cx).dirty);
        });
    }
    cx.dispatch_action(guise::actions::Undo);
    cx.read(|cx| assert_eq!(view.read(cx).editor.read(cx).text(), "Note"));
}

#[gpui::test]
fn resizing_uses_a_thin_guide_instead_of_a_scrollbar_shaped_highlight(cx: &mut TestAppContext) {
    init(cx);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view
    });
    activate(cx);
    draw(cx);
    assert!(cx.debug_bounds("sidebar-resize-guide").is_none());
    let handle = cx.debug_bounds("sidebar-resize-handle").unwrap();
    cx.simulate_mouse_move(handle.center(), None, Modifiers::none());
    cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
    draw(cx);
    let guide = cx
        .debug_bounds("sidebar-resize-guide")
        .expect("a dashed guide while resizing");
    assert_eq!(
        guide.size.width,
        px(1.0),
        "resize feedback is a hairline, not an 8 px scrollbar"
    );
    assert_eq!(guide.size.height, handle.size.height);
    cx.simulate_mouse_up(handle.center(), MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn long_outline_scrollbar_drag_reaches_the_end_without_resizing(cx: &mut TestAppContext) {
    init(cx);
    let text = (0..120)
        .map(|index| format!("## Heading {index}\n\nBody\n\n"))
        .collect::<String>();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view.sidebar_mode = SidebarMode::Outline;
        view.install(Document::untitled(&text), &text, window, cx);
        view
    });
    activate(cx);
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    let width = sidebar(cx).size.width;
    assert!(cx.debug_bounds("outline-heading-476").unwrap().top() > viewport.bottom());
    let start = point(viewport.right() - px(8.0), viewport.top() + px(12.0));
    let end = point(start.x, viewport.bottom() - px(12.0));
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    // The component scrollbar limits native drag updates to 120 Hz.
    std::thread::sleep(std::time::Duration::from_millis(12));
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    draw(cx);
    let last = cx.debug_bounds("outline-heading-476").unwrap();
    assert!(
        last.top() >= viewport.top() && last.bottom() <= viewport.bottom(),
        "dragging the scroll thumb must reveal the last heading"
    );
    assert_eq!(
        sidebar(cx).size.width,
        width,
        "vertical scrolling must not resize the sidebar"
    );
}

#[gpui::test]
fn outline_wheel_scroll_and_resize_have_separate_hit_regions(cx: &mut TestAppContext) {
    init(cx);
    let text = "# Heading\n\nBody\n\n".repeat(120);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view.sidebar_mode = SidebarMode::Outline;
        view.install(Document::untitled(&text), &text, window, cx);
        view
    });
    activate(cx);
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    let scroll_track = cx
        .debug_bounds("sidebar-scrollbar")
        .expect("visible outline scrollbar");
    let resize = cx.debug_bounds("sidebar-resize-handle").unwrap();
    assert!(
        scroll_track.right() < resize.left(),
        "the scrollbar must not overlap the resize handle"
    );
    let initial = cx.debug_bounds("outline-heading-0").unwrap().top();
    cx.simulate_mouse_move(viewport.center(), None, Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-120.0))),
        ..Default::default()
    });
    draw(cx);
    let scrolled = cx.debug_bounds("outline-heading-0").unwrap().top();
    assert!(
        scrolled < initial,
        "the outline must respond to wheel scrolling"
    );
    drag(cx, 80.0);
    assert_eq!(sidebar(cx).size.width, px(330.0));
    assert_eq!(
        cx.debug_bounds("outline-heading-0").unwrap().top(),
        scrolled,
        "resizing preserves the outline's scroll position"
    );
}

#[gpui::test]
fn outline_scroll_survives_mode_switch_and_resets_when_content_fits(cx: &mut TestAppContext) {
    init(cx);
    let text = "# Heading\n\nBody\n\n".repeat(120);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.sidebar = true;
        view.sidebar_mode = SidebarMode::Outline;
        view.install(Document::untitled(&text), &text, window, cx);
        view
    });
    activate(cx);
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-120.0))),
        ..Default::default()
    });
    draw(cx);
    let scrolled = cx.debug_bounds("outline-heading-0").unwrap().top();
    assert!(scrolled < viewport.top());
    for mode in [
        SidebarMode::Documents,
        SidebarMode::Tree,
        SidebarMode::Outline,
    ] {
        cx.update(|_, cx| view.update(cx, |this, cx| this.show_sidebar(mode, cx)));
        draw(cx);
    }
    assert_eq!(
        cx.debug_bounds("outline-heading-0").unwrap().top(),
        scrolled
    );
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.install(Document::untitled("# Short"), "# Short", window, cx);
        })
    });
    draw(cx);
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    assert_eq!(
        cx.debug_bounds("outline-heading-0").unwrap().top(),
        viewport.top()
    );
    let track = cx.debug_bounds("sidebar-scrollbar").unwrap();
    cx.simulate_click(
        point(track.center().x, track.bottom() - px(12.0)),
        Modifiers::none(),
    );
    draw(cx);
    assert_eq!(
        cx.debug_bounds("outline-heading-0").unwrap().top(),
        viewport.top(),
        "content that fits must not scroll out of view"
    );
    assert_eq!(sidebar(cx).size.width, px(250.0));
}
