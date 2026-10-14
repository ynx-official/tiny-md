use super::*;
use core::prelude::v1::test;

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn click_heading(selector: &'static str, cx: &mut VisualTestContext) {
    let row = cx.debug_bounds(selector).expect("outline heading");
    cx.simulate_click(row.center(), Modifiers::none());
    draw(cx);
    draw(cx);
}

fn heading_top(view: &Entity<TinyMd>, line: usize, cx: &mut VisualTestContext) -> f32 {
    cx.update(|window, cx| {
        view.read(cx).editor.clone().update(cx, |editor, cx| {
            let offset = editor
                .text()
                .split('\n')
                .take(line)
                .map(|line| line.encode_utf16().count() + 1)
                .sum::<usize>();
            let bounds = editor
                .bounds_for_range(offset..offset, Bounds::default(), window, cx)
                .expect("heading bounds");
            f32::from(bounds.top() - editor.scroll_handle().bounds().top())
        })
    })
}

#[gpui::test]
fn outline_navigation_aligns_headings_at_the_top_in_both_directions(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let text = (0..5)
        .map(|i| format!("## Chapter {i}\n\n{}", "正文段落 English 🌱\n\n".repeat(24)))
        .collect::<String>();
    let lines = text
        .lines()
        .enumerate()
        .filter_map(|(i, line)| line.starts_with("## ").then_some(i))
        .collect::<Vec<_>>();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        view.show_sidebar(SidebarMode::Outline, cx);
        window.activate_window();
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    draw(cx);
    for (source_mode, typewriter) in [(false, false), (true, false), (false, true)] {
        view.update(cx, |view, cx| {
            view.editor.update(cx, |editor, cx| {
                editor.set_source_mode(source_mode, cx);
                editor.set_writing_modes(false, typewriter, cx);
            });
        });
        draw(cx);
        for (line, selector) in [
            (lines[0], "outline-heading-0"),
            (lines[3], "outline-heading-150"),
            (lines[1], "outline-heading-50"),
            (lines[1], "outline-heading-50"),
            (lines[4], "outline-heading-200"),
        ] {
            click_heading(selector, cx);
            let top = heading_top(&view, line, cx);
            assert!(
                (0.0..=64.0).contains(&top),
                "heading must start near the viewport top after either jump direction: line={line}, source={source_mode}, typewriter={typewriter}, top={top}"
            );
            cx.read(|cx| {
                let view = view.read(cx);
                assert_eq!(view.editor.read(cx).model().cursor().line, line);
                assert_eq!(view.editor.read(cx).text(), text);
                assert!(!view.dirty, "outline navigation must not edit the document");
            });
            cx.update(|window, cx| {
                assert!(
                    view.read(cx)
                        .editor
                        .read(cx)
                        .focus_handle()
                        .is_focused(window)
                );
            });
        }
    }
}

#[gpui::test]
fn outline_navigation_realigns_an_already_visible_heading(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let text = format!("# First\n\n## Visible\n\n{}", "Paragraph\n\n".repeat(80));
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        view.show_sidebar(SidebarMode::Outline, cx);
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    draw(cx);
    let before = heading_top(&view, 2, cx);
    assert!(before > 64.0 && before < 400.0, "second heading is visible");
    click_heading("outline-heading-2", cx);
    let after = heading_top(&view, 2, cx);
    assert!(
        (0.0..=64.0).contains(&after),
        "visible heading must align at the top: {after}"
    );
}

#[gpui::test]
fn outline_navigation_clamps_to_the_document_end(cx: &mut TestAppContext) {
    file_open_tests::init(cx);
    let text = format!("# First\n\n{}## Last\n\nEnd", "Paragraph\n\n".repeat(80));
    let line = text.lines().position(|line| line == "## Last").unwrap();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(Document::untitled(&text), &text, window, cx);
        view.show_sidebar(SidebarMode::Outline, cx);
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    draw(cx);
    click_heading("outline-heading-162", cx);
    let scroll = cx.read(|cx| view.read(cx).editor.read(cx).scroll_handle());
    assert_eq!(scroll.offset().y, -scroll.max_offset().height);
    let top = heading_top(&view, line, cx);
    assert!(
        top >= 0.0 && top < f32::from(scroll.bounds().size.height),
        "last heading stays visible: {top}"
    );
}
