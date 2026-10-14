use super::*;
use core::prelude::v1::test;

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn caret(view: &Entity<TinyMd>, offset: usize, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.update(|window, cx| {
        view.read(cx).editor.clone().update(cx, |editor, cx| {
            editor
                .bounds_for_range(offset..offset, Bounds::default(), window, cx)
                .expect("document caret bounds")
        })
    })
}

#[gpui::test]
fn document_scrollbar_drag_freezes_diagram_geometry_until_release(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let chain = (0..12)
        .map(|i| format!("A{i} --> A{}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!(
        "Intro\n\n```mermaid\nflowchart TD\n{chain}\n```\n\n{}",
        "Paragraph\n\n".repeat(100)
    );
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        window.activate_window();
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    draw(cx);
    let scroll = cx.read(|cx| view.read(cx).editor.read(cx).scroll_handle());
    let pending_size = scroll.max_offset();
    let track = cx.debug_bounds("document-scrollbar").unwrap();
    let start = point(track.center().x, track.top() + px(12.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(1000));
    draw(cx);
    assert_eq!(
        scroll.max_offset(),
        pending_size,
        "loading must not resize the scroll range while dragging the thumb"
    );
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_ne!(
        scroll.max_offset(),
        pending_size,
        "release must apply the true diagram size"
    );
    let settled_size = scroll.max_offset();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(1000));
    draw(cx);
    assert_eq!(
        scroll.max_offset(),
        settled_size,
        "bitmap completion must not change diagram geometry"
    );
    assert_eq!(cx.read(|cx| view.read(cx).editor.read(cx).text()), text);
}

#[gpui::test]
fn document_scrollbar_drag_keeps_already_visible_diagram_errors(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let text = format!(
        "Intro\n\n```mermaid\nflowchart TD\nA[unclosed --> B\n```\n\n{}",
        "Paragraph\n\n".repeat(100)
    );
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        window.activate_window();
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(1000));
    draw(cx);
    let scroll = cx.read(|cx| view.read(cx).editor.read(cx).scroll_handle());
    let before = scroll.max_offset();
    let track = cx.debug_bounds("document-scrollbar").unwrap();
    let start = point(track.center().x, track.top() + px(12.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(
        scroll.max_offset(),
        before,
        "starting a drag must preserve an existing error's expanded source"
    );
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(scroll.max_offset(), before);
}

#[gpui::test]
fn document_scrollbar_stays_at_the_far_right_across_layouts(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let text = "Paragraph\n\n".repeat(160);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        view
    });
    for (width, sidebar, toolbar) in [(1000.0, false, false), (1800.0, true, true)] {
        view.update(cx, |view, cx| {
            view.sidebar = sidebar;
            view.toolbar = toolbar;
            cx.notify();
        });
        cx.simulate_resize(size(px(width), px(700.0)));
        draw(cx);
        draw(cx);
        let pane = cx.debug_bounds("document-editor-pane").unwrap();
        let track = cx
            .debug_bounds("document-scrollbar")
            .expect("long documents need a visible scrollbar at the window edge");
        assert_eq!(track.right(), pane.right());
        assert!(
            track.size.width <= px(16.0),
            "keep the scrollbar lane narrow"
        );
        assert_eq!(
            track.top(),
            pane.top()
                + px(if cfg!(target_os = "windows") {
                    16.0
                } else {
                    32.0
                })
        );
        assert_eq!(
            track.bottom(),
            pane.bottom() - px(if toolbar { 72.0 } else { 0.0 })
        );
    }
}

#[gpui::test]
fn document_scrollbar_drag_wheel_and_track_share_the_editor_position(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let text = "Paragraph\n\n".repeat(160);
    let last = text.encode_utf16().count();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        window.activate_window();
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    draw(cx);
    let track = cx
        .debug_bounds("document-scrollbar")
        .expect("document scrollbar");
    let start = point(track.center().x, track.top() + px(12.0));
    let end = point(start.x, track.bottom() - px(12.0));
    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    // The built-in scrollbar throttles drag updates to 120 Hz.
    std::thread::sleep(std::time::Duration::from_millis(12));
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    draw(cx);
    let last = caret(&view, last, cx);
    assert!(
        last.top() >= track.top() && last.bottom() <= track.bottom(),
        "dragging must reveal the document end"
    );

    let position = point(track.left() - px(100.0), track.center().y);
    let before = caret(&view, 0, cx).top();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.0), px(200.0))),
        ..Default::default()
    });
    draw(cx);
    assert!(
        caret(&view, 0, cx).top() > before,
        "wheel scrolling must move the same document"
    );

    cx.simulate_mouse_move(start, None, Modifiers::none());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(
        caret(&view, 0, cx).top() >= track.top(),
        "track click must return to the document start"
    );
    cx.read(|cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), text);
        assert!(!view.read(cx).dirty);
    });
}

#[gpui::test]
fn document_scrollbar_thumb_reflects_length_and_disappears_when_content_fits(
    cx: &mut TestAppContext,
) {
    file_open_tests::init(cx);
    let text = "Paragraph\n\n".repeat(40);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    draw(cx);
    let scroll = cx.read(|cx| view.read(cx).editor.read(cx).scroll_handle());
    let track = cx.debug_bounds("document-scrollbar").unwrap();
    let position = point(track.center().x, track.top() + px(75.0));
    let click = |cx: &mut VisualTestContext| {
        cx.simulate_mouse_move(position, None, Modifiers::none());
        cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
        draw(cx);
    };
    click(cx);
    assert_eq!(
        scroll.offset().y,
        px(0.0),
        "click is inside the longer thumb"
    );

    let text = text.repeat(4);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.install(Document::untitled(&text), &text, window, cx);
        });
    });
    draw(cx);
    draw(cx);
    click(cx);
    assert!(
        scroll.offset().y < px(0.0),
        "longer content must shorten the thumb so the same click lands on the track"
    );

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.install(Document::untitled("Short note"), "Short note", window, cx);
        });
    });
    draw(cx);
    draw(cx);
    click(cx);
    assert_eq!(scroll.max_offset().height, px(0.0));
    assert_eq!(
        scroll.offset().y,
        px(0.0),
        "a hidden scrollbar must not move short content"
    );
    assert!(caret(&view, 0, cx).top() >= track.top());
}

#[gpui::test]
fn document_scrollbar_works_in_source_mode_and_both_themes(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let text = "## Heading\n\nParagraph\n\n".repeat(80);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    for (dark, source) in [(false, false), (true, false), (true, true), (false, true)] {
        cx.update(|window, cx| {
            apply_theme(dark, window, cx);
            view.update(cx, |view, cx| {
                view.dark = dark;
                view.source_mode = source;
                view.editor.update(cx, |editor, cx| {
                    editor.set_style(editor_style(dark), cx);
                    editor.set_source_mode(source, cx);
                });
                cx.notify();
            });
        });
        draw(cx);
        draw(cx);
        let track = cx.debug_bounds("document-scrollbar").unwrap();
        let position = point(track.center().x, track.bottom() - px(12.0));
        cx.simulate_mouse_move(position, None, Modifiers::none());
        cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
        draw(cx);
        cx.read(|cx| {
            let editor = view.read(cx).editor.read(cx);
            assert!(editor.scroll_handle().offset().y < px(0.0));
            assert_eq!(editor.text(), text);
        });
    }
}
