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
