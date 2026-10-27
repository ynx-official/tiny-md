use super::editor_images::State;
use super::*;
use gpui::{TestAppContext, size};
use std::{path::Path, sync::Arc};

fn png(path: &Path, width: u32, height: u32) {
    image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]))
        .save(path)
        .unwrap();
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
}

fn ready(editor: &MarkdownEditor, target: &str) -> Arc<gpui::RenderImage> {
    match &editor.images.entries[target] {
        State::Ready(image) => image.clone(),
        State::Error(error) => panic!("{error}"),
        _ => panic!("image not ready"),
    }
}

#[gpui::test]
fn html_badges_share_one_centered_row_and_wrap_at_viewport_width(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let dir = tempfile::tempdir().unwrap();
    png(&dir.path().join("badge.png"), 100, 20);
    let source = "<!-- Badges -->\n<p align=\"center\">\n  <a href=\"https://example.com/release\">\n    <img src=\"badge.png\" alt=\"Version\">\n  </a>\n  <a href=\"LICENSE\">\n    <img src=\"badge.png\" alt=\"License\">\n  </a>\n  <a href=\"https://example.com/stars\"><img src=\"badge.png\" alt=\"Stars\"></a>\n</p>\n\n后文";
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = MarkdownEditor::new(cx).value(source).read_only(true);
        editor.set_document_path(Some(&dir.path().join("note.md")), cx);
        editor
    });
    let links = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = links.clone();
    cx.update(|_, cx| {
        cx.subscribe(&editor, move |_, event, _| {
            if let MarkdownEditorEvent::LinkClick(target) = event {
                observed.borrow_mut().push(target.clone());
            }
        })
        .detach();
    });
    cx.simulate_resize(size(px(640.0), px(600.0)));
    draw(cx);
    draw(cx);
    let group = cx.debug_bounds("html-image-flow").expect("badge group");
    let first = cx.debug_bounds("html-inline-image-0").unwrap();
    let second = cx.debug_bounds("html-inline-image-1").unwrap();
    let third = cx.debug_bounds("html-inline-image-2").unwrap();
    assert_eq!(first.origin.y, second.origin.y);
    assert_eq!(second.origin.y, third.origin.y);
    assert!(second.origin.x >= first.right());
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    cx.simulate_click(second.center(), gpui::Modifiers::none());
    assert_eq!(*links.borrow(), ["https://example.com/release", "LICENSE"]);
    assert!((f32::from((first.origin.x + third.right()) / 2.0 - group.center().x)).abs() < 1.0);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.layout[0].height, 0.0, "HTML comments are invisible");
        assert!(editor.layout[2..10].iter().all(|row| row.height == 0.0));
        assert_eq!(editor.text(), source);
    });
    cx.simulate_resize(size(px(250.0), px(600.0)));
    draw(cx);
    draw(cx);
    let first = cx.debug_bounds("html-inline-image-0").unwrap();
    let third = cx.debug_bounds("html-inline-image-2").unwrap();
    assert!(third.origin.y > first.origin.y, "narrow view wraps badges");
}

#[gpui::test]
fn html_badges_keep_explicit_breaks_link_actions_and_source_undo(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    cx.update(|cx| guise::Theme::light().init(cx));
    let dir = tempfile::tempdir().unwrap();
    png(&dir.path().join("badge.png"), 100, 20);
    let source = "Intro\n\n<p align='center'>\n<a href='https://example.com'><img src='badge.png'></a><br><img src='badge.png'>\n</p>\n\nAfter";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(source);
        editor.set_document_path(Some(&dir.path().join("note.md")), cx);
        window.focus(&editor.focus);
        editor
    });
    let links = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = links.clone();
    cx.update(|_, cx| {
        cx.subscribe(&editor, move |_, event, _| {
            if let MarkdownEditorEvent::LinkClick(target) = event {
                observed.borrow_mut().push(target.clone());
            }
        })
        .detach();
    });
    cx.simulate_resize(size(px(640.0), px(600.0)));
    draw(cx);
    draw(cx);
    let first = cx.debug_bounds("html-inline-image-0").unwrap();
    let second = cx.debug_bounds("html-inline-image-1").unwrap();
    assert!(second.origin.y > first.origin.y, "br forces a line break");
    cx.simulate_click(
        first.center(),
        gpui::Modifiers {
            platform: cfg!(target_os = "macos"),
            control: !cfg!(target_os = "macos"),
            ..Default::default()
        },
    );
    assert_eq!(*links.borrow(), ["https://example.com"]);
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.model.cursor().line, 3);
        assert_eq!(editor.layout[2].plan.visible, "<p align='center'>");
        assert_eq!(editor.text(), source);
    });
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "zhongwen", Some(8..8), window, cx);
            editor.replace_text_in_range(None, "中文", window, cx);
        })
    });
    draw(cx);
    assert!(cx.read(|cx| editor.read(cx).text()).contains("中文<a"));
    cx.dispatch_action(actions::Undo);
    draw(cx);
    assert_eq!(cx.read(|cx| editor.read(cx).text()), source);
    editor.update(cx, |editor, cx| {
        editor.set_source_mode(true, cx);
    });
    draw(cx);
    cx.read(|cx| {
        assert_eq!(
            editor.read(cx).layout[3].plan.visible,
            source.lines().nth(3).unwrap()
        )
    });
    editor.update(cx, |editor, cx| {
        editor.model.move_to(0, 0, false);
        editor.set_source_mode(false, cx);
    });
    draw(cx);
    cx.read(|cx| {
        assert_eq!(editor.read(cx).layout[3].height, 0.0);
        assert_eq!(editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn html_single_line_details_hides_its_image_until_opened(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "<details><summary>图片</summary><img src='missing.png'></details>";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    assert!(cx.debug_bounds("markdown-image").is_none());
    let toggle = cx.debug_bounds("html-details-toggle-0").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    draw(cx);
    assert!(cx.debug_bounds("markdown-image").is_some());
    assert_eq!(cx.read(|cx| editor.read(cx).text()), source);
}

#[gpui::test]
fn markdown_image_local_render_edit_source_resize_and_reuse(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let dir = tempfile::tempdir().unwrap();
    png(&dir.path().join("图 片.png"), 800, 400);
    let source = "Intro\n\n![本地图](<图 片.png>)\n\nAfter\n\n![重复](<图 片.png>)";
    let path = dir.path().join("note.md");
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = MarkdownEditor::new(cx).value(source);
        editor.set_document_path(Some(&path), cx);
        editor
    });
    cx.simulate_resize(size(px(640.0), px(800.0)));
    draw(cx);
    draw(cx);
    let original = cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.images.entries.len(), 1);
        assert!(editor.layout[2].text.is_none());
        assert!(editor.layout[4].y.get() >= editor.layout[2].y.get() + editor.layout[2].height);
        ready(editor, "图 片.png")
    });
    let bounds = cx
        .debug_bounds("markdown-image-pixels")
        .expect("loaded pixels rendered");
    assert!(f32::from(bounds.size.width) <= 640.0);
    assert!((f32::from(bounds.size.width) / f32::from(bounds.size.height) - 2.0).abs() < 0.01);
    // Clicking a preview reveals the same Markdown without an edit event.
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert!(editor.layout[editor.model.cursor().line].text.is_some());
        assert_eq!(editor.text(), source);
    });
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    // GPUI 0.2.2 keeps debug selectors from old frames. Check the current
    // laid-out source and its height, rather than that persistent lookup map.
    cx.read(|cx| {
        let editor = editor.read(cx);
        for line in [2, 6] {
            assert_eq!(
                editor.layout[line].plan.visible,
                source.lines().nth(line).unwrap()
            );
            assert_eq!(editor.layout[line].height, editor.layout[0].height);
            assert!(editor.layout[line].text.is_some());
        }
    });
    editor.update(cx, |editor, cx| editor.set_source_mode(false, cx));
    cx.simulate_resize(size(px(400.0), px(800.0)));
    draw(cx);
    draw(cx);
    cx.read(|cx| {
        assert!(Arc::ptr_eq(&original, &ready(editor.read(cx), "图 片.png")));
        assert_eq!(editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn markdown_image_document_path_switch_save_as_and_removal_release_old_pixels(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    png(&first.path().join("image.png"), 100, 50);
    png(&second.path().join("image.png"), 200, 150);
    let (editor, cx) = cx.add_window_view(|_, cx| {
        MarkdownEditor::new(cx)
            .value("![图](image.png)")
            .read_only(true)
    });
    draw(cx);
    cx.read(|cx| assert!(matches!(&editor.read(cx).images.entries["image.png"], State::Error(error) if error.contains("先保存"))));
    editor.update(cx, |editor, cx| {
        editor.set_document_path(Some(&first.path().join("note.md")), cx)
    });
    draw(cx);
    draw(cx);
    let old = cx.read(|cx| Arc::downgrade(&ready(editor.read(cx), "image.png")));
    editor.update(cx, |editor, cx| {
        editor.set_document_path(Some(&second.path().join("saved.md")), cx)
    });
    draw(cx);
    draw(cx);
    cx.read(|cx| assert_eq!(ready(editor.read(cx), "image.png").size(0).height.0, 150));
    assert!(old.upgrade().is_none(), "Save As releases previous pixels");
    let old = cx.read(|cx| Arc::downgrade(&ready(editor.read(cx), "image.png")));
    editor.update(cx, |editor, cx| {
        editor.apply_external_text("image removed", cx);
    });
    draw(cx);
    draw(cx);
    assert!(old.upgrade().is_none());
    cx.read(|cx| assert!(editor.read(cx).images.entries.is_empty()));
}

#[gpui::test]
fn markdown_image_missing_file_can_retry_after_file_is_created(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = MarkdownEditor::new(cx)
            .value("![Missing](image.png)")
            .read_only(true);
        editor.set_document_path(Some(&path), cx);
        editor
    });
    draw(cx);
    draw(cx);
    let retry = cx
        .debug_bounds("markdown-image-retry")
        .expect("retry offered");
    png(&dir.path().join("image.png"), 100, 50);
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    draw(cx);
    draw(cx);
    cx.read(|cx| {
        ready(editor.read(cx), "image.png");
    });
    assert!(cx.debug_bounds("markdown-image-pixels").is_some());
}

#[gpui::test]
fn markdown_image_source_only_does_not_start_downloads(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::dark().init(cx));
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor =
            MarkdownEditor::new(cx).value("![远程](https://example.invalid/image.png)");
        editor.set_source_mode(true, cx);
        editor
    });
    draw(cx);
    cx.read(|cx| assert!(editor.read(cx).images.entries.is_empty()));
    assert!(cx.debug_bounds("markdown-image").is_none());
}

#[gpui::test]
fn html_image_in_table_stays_inside_its_cell_after_loading(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let dir = tempfile::tempdir().unwrap();
    png(&dir.path().join("cell.png"), 200, 100);
    let source =
        "<table>\n<tr><td><img src='cell.png' width='80'></td><td>中文</td></tr>\n</table>";
    let path = dir.path().join("note.md");
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = MarkdownEditor::new(cx).value(source).read_only(true);
        editor.set_document_path(Some(&path), cx);
        editor
    });
    draw(cx);
    draw(cx);
    let cell = cx.debug_bounds("html-table-cell-0").unwrap();
    let image = cx.debug_bounds("markdown-image-pixels").unwrap();
    assert!(cell.contains(&image.origin));
    assert!(cell.contains(&(image.origin + point(image.size.width, image.size.height))));
    assert_eq!(image.size.width, px(80.0));
    assert_eq!(image.size.height, px(40.0));
    cx.read(|cx| {
        assert_eq!(editor.read(cx).text(), source);
        assert_eq!(editor.read(cx).layout[1].height, 0.0);
    });
}
