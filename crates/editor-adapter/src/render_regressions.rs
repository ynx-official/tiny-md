use super::*;
use gpui::TestAppContext;
use gpui::{EntityInputHandler, ScrollDelta, ScrollWheelEvent, size};

#[gpui::test]
fn typography_spacing_relayouts_cached_rows_and_preserves_source_ime_undo(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "第一段\n\n第二段\n\n\n末段\n\n```text\ncode\n```";
    let style = MarkdownStyle {
        bare: true,
        compact_blank_lines: true,
        ..Default::default()
    };
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value(source)
            .font_size(16.0)
            .style(style);
        window.focus(&editor.focus);
        window.activate_window();
        editor
    });
    draw(cx);
    draw(cx);
    editor.update(cx, |editor, cx| {
        editor.set_style(
            MarkdownStyle {
                line_height: Some(2.0),
                paragraph_spacing: Some(0.75),
                ..style
            },
            cx,
        )
    });
    draw(cx);
    draw(cx);
    let position = cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.layout[0].height, 32.0);
        assert_eq!(editor.layout[1].height, 24.0);
        assert_eq!(editor.layout[2].y.get(), 56.0);
        assert_eq!(
            editor.layout[3].height, 32.0,
            "explicit blank runs keep a full line"
        );
        assert_eq!(
            editor.layout[8].line_h, 22.0,
            "code keeps its dedicated line height"
        );
        assert_eq!(editor.text(), source);
        editor.text_bounds.origin + point(px(2.0), px(44.0))
    });
    cx.simulate_click(position, gpui::Modifiers::none());
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_eq!(editor.model.cursor(), Pos::new(1, 0));
            editor.replace_and_mark_text_in_range(None, "pinyin", Some(6..6), window, cx);
            editor.set_style(
                MarkdownStyle {
                    line_height: Some(1.8),
                    paragraph_spacing: Some(1.0),
                    ..style
                },
                cx,
            );
            assert!(editor.is_composing());
        })
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_painted_caret_at_cursor(editor, window, cx);
            editor.replace_text_in_range(None, "中文🌱", window, cx);
        })
    });
    draw(cx);
    editor.update(cx, |editor, cx| editor.history(false, cx));
    draw(cx);
    cx.read(|cx| {
        assert_eq!(editor.read(cx).text(), source);
        assert_eq!(editor.read(cx).layout[1].height, 29.0);
    });
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    cx.read(|cx| assert!(editor.read(cx).layout.iter().all(|row| row.line_h == 22.0)));
    editor.update(cx, |editor, cx| {
        editor.set_source_mode(false, cx);
        editor.set_style(style, cx);
    });
    draw(cx);
    cx.read(|cx| {
        assert_eq!(editor.read(cx).layout[1].height, 13.0);
        assert_eq!(editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn table_toolbar_tracks_cursor_without_changing_document_or_row_positions(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "正文\n\n| 标题 | 内容 |\n| --- | --- |\n| 中文 | 内容 |\n\n后文";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(source);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("table-toolbar").is_none());
    let before = cx.read(|app| editor.read(app).layout[6].y.get());
    editor.update(cx, |editor, cx| {
        editor.model.move_to(4, 3, false);
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("table-toolbar").is_some(),
        "the active table needs local actions"
    );
    cx.read(|app| {
        let editor = editor.read(app);
        assert_eq!(editor.text(), source);
        assert_eq!(editor.layout[6].y.get(), before);
    });
}

#[gpui::test]
fn functional_html_marks_have_one_clickable_row_and_keep_ime_undo(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "正文\n<br/>\n<!-- hello 中文🌱 -->\n后文";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(source);
        window.focus(&editor.focus);
        editor
    });
    cx.update(|window, _| window.activate_window());
    draw(cx);
    let position = cx.read(|app| {
        let editor = editor.read(app);
        for line in [1, 2] {
            let row = &editor.layout[line];
            assert_eq!(row.plan.visible, source.lines().nth(line).unwrap());
            assert_eq!(row.visual_rows(), 1);
            assert_eq!(row.height, editor.layout[3].height);
        }
        let row = &editor.layout[2];
        editor.text_bounds.origin + point(px(2.0), px(row.y.get() + row.height / 2.0))
    });
    cx.simulate_click(position, gpui::Modifiers::none());
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_eq!(editor.model.cursor().line, 2);
            editor.edit(cx, |model| model.move_to(2, "<!-- ".chars().count(), false));
            editor.replace_and_mark_text_in_range(None, "pinyin", Some(6..6), window, cx);
        })
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_painted_caret_at_cursor(editor, window, cx);
            editor.replace_text_in_range(None, "输入法", window, cx);
        })
    });
    draw(cx);
    cx.read(|cx| {
        assert_eq!(
            editor.read(cx).text(),
            source.replace("hello", "输入法hello")
        )
    });
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    editor.update(cx, |editor, cx| editor.set_source_mode(false, cx));
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|cx| assert_eq!(editor.read(cx).text(), source));
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| {
            model.move_to(1, 0, false);
            model.move_to(2, source.lines().nth(2).unwrap().chars().count(), true);
        })
    });
    draw(cx);
    cx.read(|cx| {
        assert_eq!(
            editor.read(cx).model.copy().as_deref(),
            Some("<br/>\n<!-- hello 中文🌱 -->")
        )
    });
}

#[gpui::test]
fn functional_html_marks_keep_neutrino_sample_layout(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    // Git may check this fixture out with CRLF on Windows. The editor normalizes
    // line endings, so compare and render against the canonical LF source.
    let source = include_str!("../../../fixtures/html-functional-markers.md").replace("\r\n", "\n");
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = MarkdownEditor::new(cx).value(&source).read_only(true);
        let doc = crate::html::Document::parse(&source, crate::html::parse_root(&source).as_ref());
        // Decode/load behavior has separate tests; fixed image sizes keep this layout
        // regression deterministic while retaining every original URL and source byte.
        for image in doc.image_tags() {
            let target = &image.attrs["src"];
            let (w, h) = if target.starts_with("assets/") {
                (720, 180)
            } else {
                (100, 20)
            };
            let buffer = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
            editor.images.entries.insert(
                target.clone(),
                editor_images::State::Ready(std::sync::Arc::new(gpui::RenderImage::new(vec![
                    image::Frame::new(buffer),
                ]))),
            );
        }
        editor
    });
    cx.simulate_resize(size(px(900.0), px(1200.0)));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        for (i, line) in source.lines().enumerate() {
            if line.starts_with("<!--") || line == "<br/>" {
                let row = &editor.layout[i];
                assert_eq!(row.plan.visible, line);
                assert!(row.height > 0.0);
                assert_eq!(row.visual_rows(), 1);
            }
        }
        assert_eq!(editor.text(), source);
        assert!(matches!(editor.layout[7].plan.kind, RowKind::Heading(3)));
        assert_eq!(editor.layout[7].align, TextAlign::Center);
        let parsed = editor.document_cache.current().unwrap();
        assert_eq!(parsed.html.image_groups.len(), 2);
        assert_eq!(parsed.html.image_groups[1].align, 1);
        assert_eq!(parsed.html.image_tags().count(), 5);
    });
    let second = cx.debug_bounds("html-inline-image-1").unwrap();
    let third = cx.debug_bounds("html-inline-image-2").unwrap();
    let fourth = cx.debug_bounds("html-inline-image-3").unwrap();
    assert_eq!(second.origin.y, third.origin.y);
    assert_eq!(third.origin.y, fourth.origin.y);
}

#[gpui::test]
fn html_headings_match_markdown_levels_without_changing_source(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = (1..=6)
        .flat_map(|level| {
            [
                format!("<h{level}>中文标题 🌱</h{level}>"),
                String::new(),
                format!("{} 中文标题 🌱", "#".repeat(level)),
                String::new(),
            ]
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(&source).read_only(true));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        for level in 1..=6 {
            let html = &editor.layout[(level - 1) * 4];
            let markdown = &editor.layout[(level - 1) * 4 + 2];
            assert_eq!(html.plan.visible, "中文标题 🌱");
            assert!(matches!(html.plan.kind, RowKind::Heading(n) if n == level as u8));
            assert_eq!(html.line_h, markdown.line_h);
            assert_eq!(html.pad_top, markdown.pad_top);
            assert_eq!(html.height, markdown.height);
        }
        assert_eq!(editor.text(), source);
    });
}

#[gpui::test]
fn html_headings_support_multiline_content_and_nested_styles(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "<h3 align='center'>\n    <em>中文</em> &amp; 标题\n</h3>\n\n正文";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.layout[0].height, 0.0);
        assert_eq!(editor.layout[2].height, 0.0);
        let row = &editor.layout[1];
        assert_eq!(row.plan.visible.trim(), "中文 & 标题");
        assert!(matches!(row.plan.kind, RowKind::Heading(3)));
        assert_eq!(row.align, TextAlign::Center);
        assert!(
            row.plan
                .runs
                .iter()
                .any(|run| run.style.bold && run.style.italic)
        );
        assert!(matches!(editor.layout[4].plan.kind, RowKind::Paragraph));
        assert_eq!(editor.text(), source);
    });
    // The content line is unchanged, but the surrounding heading level changed.
    let before = cx.read(|cx| editor.read(cx).layout[1].line_h);
    editor.update(cx, |editor, cx| {
        editor.apply_external_text(&source.replace("h3", "h2"), cx);
    });
    draw(cx);
    cx.read(|cx| {
        let row = &editor.read(cx).layout[1];
        assert!(matches!(row.plan.kind, RowKind::Heading(2)));
        assert!(row.line_h > before);
    });
}

#[gpui::test]
fn html_headings_keep_chinese_composition_source_mode_and_undo(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "Intro\n\n<h3 align='center'>原文🌱</h3>";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(source);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    cx.read(|cx| {
        assert!(matches!(
            editor.read(cx).layout[2].plan.kind,
            RowKind::Heading(3)
        ));
    });
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| {
            model.move_to(2, "<h3 align='center'>".chars().count(), false)
        });
    });
    draw(cx);
    cx.read(|cx| {
        assert_eq!(
            editor.read(cx).layout[2].plan.visible,
            source.lines().nth(2).unwrap()
        );
    });
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "pinyin", Some(6..6), window, cx);
        });
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            let range = editor.ime.marked().unwrap();
            assert!(
                editor
                    .bounds_for_range(range, Bounds::default(), window, cx)
                    .is_some()
            );
            editor.replace_text_in_range(None, "中文", window, cx);
        });
    });
    draw(cx);
    cx.read(|cx| assert_eq!(editor.read(cx).text(), source.replace("原文", "中文原文")));
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    cx.read(|cx| assert!(editor.read(cx).layout[2].plan.visible.contains("<h3")));
    editor.update(cx, |editor, cx| editor.set_source_mode(false, cx));
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|cx| assert_eq!(editor.read(cx).text(), source));
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| model.move_to(0, 0, false))
    });
    draw(cx);
    cx.read(|cx| {
        let row = &editor.read(cx).layout[2];
        assert_eq!(row.plan.visible, "原文🌱");
        assert!(matches!(row.plan.kind, RowKind::Heading(3)));
    });
}

#[gpui::test]
fn html_inline_preview_preserves_source_and_renders_text(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "中文 <strong>加粗</strong> &amp; <u>下划线</u><br>下一行";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.layout[0].plan.visible, "中文 加粗 & 下划线\n下一行");
        assert_eq!(editor.layout[0].visual_rows(), 2);
        assert_eq!(editor.text(), source);
    });
}

#[gpui::test]
fn html_details_toggle_only_changes_view(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "<details>\n<summary>中文标题</summary>\n正文 <b>加粗</b>\n</details>\n后文";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    cx.read(|cx| assert_eq!(editor.read(cx).layout[2].height, 0.0));
    let toggle = cx
        .debug_bounds("html-details-toggle-0")
        .expect("details toggle");
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    draw(cx);
    cx.read(|cx| {
        assert!(editor.read(cx).layout[2].height > 0.0);
        assert_eq!(editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn html_table_preview_reveals_original_source_for_editing(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "Intro\n<table>\n<tr><th colspan='2'>标题</th></tr>\n<tr><td>中文<br>换行</td><td>B</td></tr>\n</table>\nAfter";
    let (editor, cx) = cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source));
    draw(cx);
    let table = cx
        .debug_bounds("html-table-cell-0")
        .expect("HTML cell preview");
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.layout[2].height, 0.0);
        assert!(editor.layout[1].height > editor.layout[0].height * 2.0);
        assert_eq!(editor.text(), source);
    });
    cx.simulate_click(table.center(), gpui::Modifiers::none());
    draw(cx);
    cx.read(|cx| {
        assert!(editor.read(cx).layout[2].height > 0.0);
        assert_eq!(editor.read(cx).text(), source);
    });
}

#[gpui::test]
fn html_table_preserves_literal_text_and_inherits_container_style(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source =
        "<table style='font-style:italic'><tr><td>**原文** &amp;</td><td>普通字</td></tr></table>";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        let table = editor.layout[0].html_table.as_ref().unwrap();
        assert_eq!(table.cells[0].row.plan.visible, "**原文** &");
        assert!(
            table
                .cells
                .iter()
                .all(|cell| cell.row.plan.runs.iter().all(|run| run.style.italic))
        );
        assert_eq!(editor.text(), source);
    });
}

#[gpui::test]
fn html_chinese_composition_source_switch_and_undo_preserve_tags(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source =
        "中文 <span style='color:#c00'>原文🌱</span> &amp;\n<table><tr><td>内容</td></tr></table>";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(source);
        editor
            .model
            .move_to(0, "中文 <span style='color:#c00'>".chars().count(), false);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "pinyin", Some(6..6), window, cx);
        })
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            let range = editor.ime.marked().unwrap();
            assert!(
                editor
                    .bounds_for_range(range, Bounds::default(), window, cx)
                    .is_some()
            );
            editor.replace_text_in_range(None, "输入法", window, cx);
        })
    });
    draw(cx);
    cx.read(|cx| {
        assert_eq!(
            editor.read(cx).text(),
            source.replacen("原文", "输入法原文", 1)
        )
    });
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    editor.update(cx, |editor, cx| editor.set_source_mode(false, cx));
    draw(cx);
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|cx| assert_eq!(editor.read(cx).text(), source));
}

#[gpui::test]
fn html_details_hides_markdown_fences_and_single_line_body(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "<details>\n<summary>代码</summary>\n\n```rust\nlet x = 1;\n```\n\n</details>\n<details><summary>摘要</summary>隐藏内容</details>";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        for line in 2..7 {
            assert_eq!(editor.layout[line].height, 0.0, "line {line}");
        }
        assert_eq!(editor.layout[8].plan.visible, "摘要");
    });
}

#[gpui::test]
fn html_table_content_edit_invalidates_preview_and_multiline_summary_is_toggleable(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "<table>\n<tr><td>AAA</td></tr>\n</table>\n<details>\n<summary>\n标题\n</summary>\n内容\n</details>";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    assert!(cx.debug_bounds("html-details-toggle-0").is_some());
    editor.update(cx, |editor, cx| {
        editor.apply_external_text(&source.replace("AAA", "BBB"), cx);
    });
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(
            editor.layout[0].html_table.as_ref().unwrap().cells[0]
                .row
                .plan
                .visible,
            "BBB"
        );
        assert_eq!(editor.layout[7].height, 0.0);
    });
}

#[gpui::test]
fn markdown_image_preview_is_present_in_formatted_mode(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "![方案阅读方式对比](../02-design/assets/comparison.png)";
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(source).read_only(true));
    draw(cx);
    assert!(
        cx.debug_bounds("markdown-image").is_some(),
        "formatted images need a preview or an explicit loading/error state"
    );
    assert_eq!(cx.read(|cx| editor.read(cx).text()), source);
}

#[gpui::test]
fn diagram_document_reopening_regenerates_after_releasing_idle_images(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = std::env::var_os("TINY_MD_DIAGRAM_DOCUMENT")
        .map(|path| std::fs::read_to_string(path).expect("read diagram document"))
        .unwrap_or_else(|| "Intro\n\n```mermaid\nflowchart LR\nA --> B\n```\n\n```mermaid\nflowchart LR\nA[unclosed --> B\n```".into());
    let (editor, cx) = cx.add_window_view(|_, cx| {
        MarkdownEditor::new(cx)
            .value(&text)
            .read_only(true)
            .style(MarkdownStyle {
                bare: true,
                ..Default::default()
            })
    });
    cx.simulate_resize(size(px(1064.0), px(700.0)));
    for _ in 0..3 {
        draw(cx);
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
    }
    draw(cx);
    let originals = cx.read(|cx| {
        let editor = editor.read(cx);
        editor
            .document_cache
            .current()
            .unwrap()
            .code
            .iter()
            .filter(|block| block.is_mermaid())
            .map(|block| {
                let key = DiagramKey {
                    source: block.source.clone(),
                    dark: false,
                };
                let state = match &editor.diagrams[&key] {
                    DiagramState::Ready(diagram) => Ok(std::sync::Arc::downgrade(&diagram.image)),
                    DiagramState::Error(error) => Err(error.clone()),
                    _ => panic!("initial diagrams must finish"),
                };
                (
                    key,
                    state,
                    editor.diagram_views[&block.start].preview_id(cx),
                    editor.diagram_views[&block.start].preview_reference(cx),
                )
            })
            .collect::<Vec<_>>()
    });
    let capacity = cx.update(|_, cx| crate::diagram_work::layout(cx));
    let permits = crate::diagram_work::occupy(&capacity);
    assert_eq!(permits.len(), crate::diagram_work::MAX_CONCURRENT);
    let retained_bytes = || {
        let mut unique = std::collections::HashSet::new();
        originals
            .iter()
            .filter_map(|(_, _, _, pixels)| pixels.as_ref()?.upgrade())
            .filter(|pixels| unique.insert(pixels.id))
            .map(|pixels| pixels.as_bytes(0).unwrap().len())
            .sum::<usize>()
    };
    let open_bytes = retained_bytes();
    editor.update(cx, |editor, cx| editor.set_text("Another document", cx));
    draw(cx);
    draw(cx);
    let idle_bytes = retained_bytes();
    println!(
        "diagram_idle_memory: diagrams={} active_pixel_mib={:.2} retired_pixel_mib={:.2}",
        originals.len(),
        open_bytes as f64 / 1_048_576.0,
        idle_bytes as f64 / 1_048_576.0
    );
    assert_eq!(
        idle_bytes, 0,
        "switching documents must not retain idle CPU pixels"
    );
    assert!(
        originals.iter().all(|(_, svg, _, _)| match svg {
            Ok(svg) => svg.upgrade().is_none(),
            Err(_) => true,
        }),
        "switching documents must release unowned SVG data too"
    );
    editor.update(cx, |editor, cx| editor.set_text(&text, cx));
    draw(cx);
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        for (key, _, _, _) in &originals {
            assert!(
                matches!(editor.diagrams[key], DiagramState::Loading { .. }),
                "reopening a retired diagram must schedule fresh layout work"
            );
        }
        assert_eq!(editor.text(), text);
    });
    drop(permits);
    for _ in 0..3 {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
        draw(cx);
    }
    cx.read(|cx| {
        let editor = editor.read(cx);
        let blocks = editor.document_cache.current().unwrap();
        for ((key, original, image_id, _), block) in originals.iter().zip(blocks.code.iter().filter(|block| block.is_mermaid())) {
            match (original, &editor.diagrams[key]) {
                (Ok(_), DiagramState::Ready(_)) => {
                    assert!(image_id.is_some(), "initial valid diagrams must have pixels");
                    assert!(editor.diagram_views[&block.start].preview_id(cx).is_some(), "all diagrams, including offscreen rows, must regenerate their pixels");
                    assert_ne!(editor.diagram_views[&block.start].preview_id(cx), *image_id, "retired pixels must be newly decoded");
                }
                (Err(original), DiagramState::Error(error)) => assert_eq!(original, error),
                _ => panic!("reopening must regenerate the same diagram or error"),
            }
        }
        println!("diagram_idle_reopen: diagrams={} bitmaps={} max_concurrent={} all_offscreen_previews_retained=true", originals.len(), originals.iter().filter(|(_, state, _, _)| state.is_ok()).count(), crate::diagram_work::MAX_CONCURRENT);
        assert_eq!(editor.text(), text);
    });
}

#[gpui::test]
fn diagram_cache_text_edits_only_rerender_changed_sources_and_theme_roundtrips_reuse_svg(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text =
        "Intro\n\n```mermaid\nflowchart LR\nA --> B\n```\n\n```mermaid\nflowchart TD\nC --> D\n```";
    let sources = ["flowchart LR\nA --> B", "flowchart TD\nC --> D"];
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(text).read_only(true));
    for _ in 0..3 {
        draw(cx);
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
    }
    draw(cx);
    let originals = cx.read(|cx| {
        sources.map(|source| {
            let key = DiagramKey {
                source: source.into(),
                dark: false,
            };
            let DiagramState::Ready(diagram) = &editor.read(cx).diagrams[&key] else {
                panic!("initial diagram")
            };
            diagram.image.clone()
        })
    });
    let capacity = cx.update(|_, cx| crate::diagram_work::layout(cx));
    let permit = crate::diagram_work::occupy(&capacity);
    editor.update(cx, |editor, cx| {
        editor.apply_external_text(&text.replace("Intro", "Edited prose"), cx)
    });
    draw(cx);
    cx.read(|cx| {
        for (index, source) in sources.iter().enumerate() {
            let key = DiagramKey {
                source: (*source).into(),
                dark: false,
            };
            let DiagramState::Ready(diagram) = &editor.read(cx).diagrams[&key] else {
                panic!("prose must reuse diagram")
            };
            assert!(std::sync::Arc::ptr_eq(&originals[index], &diagram.image));
        }
    });
    let edited = text.replace("A --> B", "A --> Changed");
    editor.update(cx, |editor, cx| editor.apply_external_text(&edited, cx));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        let unchanged = DiagramKey {
            source: sources[1].into(),
            dark: false,
        };
        let DiagramState::Ready(diagram) = &editor.diagrams[&unchanged] else {
            panic!("other graph must stay ready")
        };
        assert!(std::sync::Arc::ptr_eq(&originals[1], &diagram.image));
        assert!(matches!(
            editor.diagrams[&DiagramKey {
                source: "flowchart LR\nA --> Changed".into(),
                dark: false
            }],
            DiagramState::Loading { .. }
        ));
    });
    drop(permit);
    for _ in 0..3 {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
        draw(cx);
    }
    cx.update(|_, cx| guise::Theme::dark().init(cx));
    editor.update(cx, |_, cx| cx.notify());
    for _ in 0..3 {
        draw(cx);
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
    }
    cx.read(|cx| {
        let dark_key = DiagramKey {
            source: sources[1].into(),
            dark: true,
        };
        let DiagramState::Ready(diagram) = &editor.read(cx).diagrams[&dark_key] else {
            panic!("dark diagram")
        };
        assert_ne!(
            diagram.image.bytes(),
            originals[1].bytes(),
            "themes must use separate cache entries"
        );
    });
    let _permit = crate::diagram_work::occupy(&capacity);
    cx.update(|_, cx| guise::Theme::light().init(cx));
    editor.update(cx, |editor, cx| {
        editor.apply_external_text(text, cx);
        cx.notify();
    });
    draw(cx);
    cx.read(|cx| {
        for (index, source) in sources.iter().enumerate() {
            let key = DiagramKey {
                source: (*source).into(),
                dark: false,
            };
            let DiagramState::Ready(diagram) = &editor.read(cx).diagrams[&key] else {
                panic!("theme and undo must reuse the cached SVG")
            };
            assert!(std::sync::Arc::ptr_eq(&originals[index], &diagram.image));
        }
    });
}

#[gpui::test(iterations = 10)]
fn diagram_open_order_follows_the_document(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let capacity = cx.update(crate::diagram_work::layout);
    let permit = crate::diagram_work::occupy(&capacity);
    let sources = (0..8)
        .map(|i| format!("flowchart LR\nA{i} --> B{i}"))
        .collect::<Vec<_>>();
    let text = sources
        .iter()
        .map(|source| format!("```mermaid\n{source}\n```\n\n"))
        .collect::<String>();
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(&text).read_only(true));
    let order = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = order.clone();
    let keys = sources
        .iter()
        .map(|source| DiagramKey {
            source: source.clone(),
            dark: false,
        })
        .collect::<Vec<_>>();
    cx.update(|_, cx| {
        cx.observe(&editor, move |editor, cx| {
            let editor = editor.read(cx);
            let mut order = observed.borrow_mut();
            for (index, key) in keys.iter().enumerate() {
                if matches!(editor.diagrams.get(key), Some(DiagramState::Ready(_)))
                    && !order.contains(&index)
                {
                    order.push(index);
                }
            }
        })
        .detach();
    });
    draw(cx);
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(181));
    cx.run_until_parked();
    drop(permit);
    for _ in 0..3 {
        draw(cx);
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
    }
    draw(cx);
    assert_eq!(
        *order.borrow(),
        (0..8).collect::<Vec<_>>(),
        "diagrams must appear from the start to the end of the document"
    );
}

#[gpui::test]
fn diagram_open_keeps_later_cached_duplicates_behind_pending_diagrams(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    // Hold only the missing layout so the cached first SVG can still decode
    // using the shared CPU slots.
    let pending = DiagramKey {
        source: "flowchart TD\nC --> D".into(),
        dark: false,
    };
    let permit = cx
        .update(|cx| crate::diagram_cache::request_layout(cx, &pending))
        .try_acquire();
    let first = "flowchart LR\nA --> B";
    let key = DiagramKey {
        source: first.into(),
        dark: false,
    };
    let cached = diagrams::render(&key).unwrap();
    let text = format!(
        "```mermaid\n{first}\n```\n\n```mermaid\nflowchart TD\nC --> D\n```\n\n```mermaid\n{first}\n```"
    );
    let (editor, cx) = cx.add_window_view(|_, cx| {
        let mut editor = MarkdownEditor::new(cx).value(&text).read_only(true);
        editor.diagrams.insert(key, DiagramState::Ready(cached));
        editor
    });
    draw(cx);
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(1000));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        let blocks = &editor.document_cache.current().unwrap().code;
        assert!(
            editor.diagram_views[&blocks[0].start]
                .preview_size(cx)
                .is_some()
        );
        assert!(
            editor.diagram_views[&blocks[2].start]
                .preview_size(cx)
                .is_none(),
            "a later cached SVG must not jump the document's decode order"
        );
    });
    drop(permit);
    for _ in 0..3 {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
        draw(cx);
    }
    assert!(cx.read(|cx| {
        editor
            .read(cx)
            .diagram_views
            .values()
            .all(|view| view.preview_size(cx).is_some())
    }));
}

#[gpui::test(iterations = 5)]
fn diagram_cache_parallel_documents_share_the_same_layout(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let key = DiagramKey {
        source: "flowchart LR\nA[Shared] --> B[Layout]".into(),
        dark: false,
    };
    let text = format!("```mermaid\n{}\n```", key.source);
    let editors = (0..5)
        .map(|_| {
            let (editor, visual) = cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(&text));
            draw(visual);
            editor
        })
        .collect::<Vec<_>>();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(181));
    cx.run_until_parked();
    cx.read(|cx| {
        let images = editors
            .iter()
            .map(|editor| match editor.read(cx).diagrams.get(&key) {
                Some(DiagramState::Ready(diagram)) => diagram.image.clone(),
                _ => panic!("every parallel document must finish its layout"),
            })
            .collect::<Vec<_>>();
        assert!(
            images
                .iter()
                .all(|image| std::sync::Arc::ptr_eq(&images[0], image)),
            "simultaneous cache misses must reuse one generated SVG"
        );
    });
}

#[gpui::test]
fn diagram_open_skeleton_and_height_changes_keep_the_reading_anchor(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let capacity = cx.update(crate::diagram_work::layout);
    let permit = crate::diagram_work::occupy(&capacity);
    let source = format!(
        "flowchart TD\n{}",
        (0..10)
            .map(|i| format!("A{i} --> A{}", i + 1))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let text = format!(
        "Intro\n\n```mermaid\n{source}\n```\n\n{}Reading target\n{}",
        "before\n".repeat(40),
        "after\n".repeat(100)
    );
    let target = text
        .lines()
        .position(|line| line == "Reading target")
        .unwrap();
    let (editor, cx) =
        cx.add_window_view(|_, cx| MarkdownEditor::new(cx).value(&text).read_only(true));
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    draw(cx);
    let skeleton = cx
        .debug_bounds("diagram-skeleton")
        .expect("pending diagrams need a skeleton");
    assert!(
        skeleton.size.height >= px(200.0),
        "reserve space instead of a 72 px loading label"
    );
    let pending_height = cx.read(|cx| editor.read(cx).layout[2].height);
    editor.update(cx, |editor, cx| {
        let offset = 100.0 - editor.layout[target].y.get();
        editor.scroll.set_offset(point(px(0.0), px(offset)));
        cx.notify();
    });
    draw(cx);
    let before = cx.read(|cx| {
        let editor = editor.read(cx);
        editor.layout[target].y.get() + f32::from(editor.scroll.offset().y)
    });
    drop(permit);
    for _ in 0..3 {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
        draw(cx);
    }
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_ne!(
            editor.layout[2].height, pending_height,
            "the test must exercise a real size correction"
        );
        let after = editor.layout[target].y.get() + f32::from(editor.scroll.offset().y);
        assert!(
            (before - after).abs() < 0.5,
            "diagram loading moved the reading target: {before} -> {after}"
        );
        assert_eq!(editor.text(), text);
    });
}

#[gpui::test]
fn diagram_layout_waits_for_capacity_and_cancels_replaced_documents(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let queue = cx.update(crate::diagram_work::layout);
    let permit = crate::diagram_work::occupy(&queue);
    let old_text = (0..12)
        .map(|i| format!("```mermaid\nflowchart LR\nA{i} --> B{i}\n```\n\n"))
        .collect::<String>();
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx)
            .value(&old_text)
            .style(MarkdownStyle {
                bare: true,
                ..Default::default()
            });
        editor.set_read_only(true, cx);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(1000));
    draw(cx);
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.diagrams.len(), 12);
        assert!(
            editor
                .diagrams
                .values()
                .all(|state| matches!(state, DiagramState::Loading { .. })),
            "no Mermaid layout may run while the shared worker is occupied"
        );
    });
    let new_text = "replacement\n\n```mermaid\nflowchart TD\nNew --> Document\n```";
    editor.update(cx, |editor, cx| editor.set_text(new_text, cx));
    draw(cx);
    drop(permit);
    for _ in 0..3 {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
        draw(cx);
    }
    cx.read(|cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.text(), new_text);
        assert_eq!(editor.diagrams.len(), 1);
        assert!(
            editor
                .diagrams
                .values()
                .all(|state| matches!(state, DiagramState::Ready(_)))
        );
        assert!(
            editor
                .diagram_views
                .values()
                .all(|view| view.preview_size(cx).is_some())
        );
    });
    assert!(
        crate::diagram_work::occupy(&queue).len() == crate::diagram_work::MAX_CONCURRENT,
        "cancellation must release capacity"
    );
}

#[gpui::test]
fn all_diagrams_finish_decoding_after_open_without_any_scroll(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = std::env::var_os("TINY_MD_DIAGRAM_DOCUMENT")
        .map(|path| std::fs::read_to_string(path).expect("read diagram document"))
        .unwrap_or_else(|| format!("paragraph\n{}\n```mermaid\nflowchart LR\nA --> B\n```\n\n```mermaid\nflowchart TD\nC --> D\n```\n\n```mermaid\nflowchart LR\nA --> B\n```", "offscreen text\n".repeat(200)));
    let expected =
        crate::code_blocks::collect(&text.lines().map(str::to_owned).collect::<Vec<_>>())
            .0
            .iter()
            .filter(|block| block.is_mermaid())
            .count();
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(&text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        editor.set_read_only(true, cx);
        window.focus(&editor.focus);
        editor
    });
    cx.simulate_resize(size(px(1064.0), px(700.0)));
    let dpi = cx.update(|window, _| window.scale_factor());
    for _ in 0..3 {
        draw(cx);
        draw(cx);
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1000));
        cx.run_until_parked();
    }
    cx.read(|cx| {
        let editor = editor.read(cx);
        let parsed = editor.document_cache.current().unwrap();
        let states: Vec<_> = parsed.code.iter().filter(|block| block.is_mermaid())
            .filter_map(|block| editor.diagrams.get(&DiagramKey { source: block.source.clone(), dark: false })).collect();
        let ready = states.iter().filter(|state| matches!(state, DiagramState::Ready(_))).count();
        let errors = states.iter().filter(|state| matches!(state, DiagramState::Error(_))).count();
        let decoded: Vec<_> = editor.diagram_views.values().filter_map(|view| view.preview_size(cx)).collect();
        assert_eq!(ready + errors, expected);
        assert_eq!(decoded.len(), ready, "every valid diagram must have decoded pixels before scrolling");
        println!("mermaid_blocks={expected} ready={ready} errors={errors} decoded={} dpi={dpi} pixel_mib={:.2}", decoded.len(), decoded.iter().map(|size| size.bytes()).sum::<usize>() as f64 / 1048576.0);
        assert_eq!(editor.text(), text);
    });
}

#[gpui::test]
fn opening_the_document_decodes_offscreen_diagrams_without_scrolling(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "flowchart LR\nA --> B";
    let key = DiagramKey {
        source: source.into(),
        dark: false,
    };
    let diagram = diagrams::render(&key).unwrap();
    let text = format!(
        "paragraph\n{}\n```mermaid\n{source}\n```",
        "offscreen text\n".repeat(200)
    );
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(&text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        editor.diagrams.insert(key, DiagramState::Ready(diagram));
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(101));
    cx.run_until_parked();
    cx.read(|cx| {
        let editor = editor.read(cx);
        let view = editor.diagram_views.values().next().unwrap();
        assert!(view.preview_size(cx).is_some(), "opening must finish pixel decoding for a diagram outside the viewport without scrolling");
    });
}

#[gpui::test]
fn diagram_view_button_emits_a_snapshot_without_editing_the_document(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = "paragraph\n\n```mermaid\nflowchart LR\nA --> B\n```";
    let key = DiagramKey {
        source: "flowchart LR\nA --> B".into(),
        dark: false,
    };
    let diagram = diagrams::render(&key).unwrap();
    let image = diagram.image.clone();
    let snapshot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        editor.diagrams.insert(key, DiagramState::Ready(diagram));
        window.focus(&editor.focus);
        editor
    });
    let result = snapshot.clone();
    cx.update(|_, cx| {
        cx.subscribe(&editor, move |_, event, _| {
            if let MarkdownEditorEvent::ViewDiagram(diagram) = event {
                *result.borrow_mut() = Some(diagram.clone());
            }
        })
        .detach();
    });
    draw(cx);
    draw(cx);
    let button = cx
        .debug_bounds("diagram-view")
        .expect("diagram has a view button");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    assert!(std::sync::Arc::ptr_eq(
        &image,
        &snapshot
            .borrow()
            .as_ref()
            .expect("view snapshot emitted")
            .image
    ));
    assert_eq!(cx.read(|cx| editor.read(cx).text()), text);
}

#[gpui::test]
fn long_document_scroll_and_paragraph_edit_have_bounded_layout_work(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    // Optional local measurement input; the default remains portable in CI.
    let text = std::env::var_os("TINY_MD_PERFORMANCE_DOCUMENT")
        .map(|path| std::fs::read_to_string(path).expect("read performance document"))
        .unwrap_or_else(|| include_str!("../../../fixtures/product-definition-v2.md").to_owned());
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(&text).style(MarkdownStyle {
            bare: true,
            compact_headings: true,
            ..Default::default()
        });
        window.focus(&editor.focus);
        editor
    });
    cx.simulate_resize(size(px(1080.0), px(700.0)));
    draw(cx);
    draw(cx);
    let ready = cx.read(|app| editor.read(app).render_work());
    let lines = cx.read(|app| editor.read(app).model.line_count());
    assert_eq!(
        cx.read(|app| editor
            .read(app)
            .document_cache
            .current()
            .unwrap()
            .tables
            .len()),
        24
    );
    // The caret can move without modifying the model or the table layout.
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| model.move_to(1, 0, false))
    });
    draw(cx);
    let stable = cx.read(|app| editor.read(app).render_work());
    cx.simulate_mouse_move(point(px(100.0), px(100.0)), None, gpui::Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(100.0), px(100.0)),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-4000.0))),
        ..Default::default()
    });
    draw(cx);
    let scrolled = cx.read(|app| editor.read(app).render_work());
    assert_eq!(scrolled.shaped_rows, stable.shaped_rows);
    assert_eq!(scrolled.shaped_cells, stable.shaped_cells);
    assert!(scrolled.built_rows - stable.built_rows < lines as u64 / 3);
    let edit_line = cx.read(|app| {
        let editor = editor.read(app);
        let parsed = editor.document_cache.current().unwrap();
        (0..lines)
            .find(|&line| {
                parsed.blocks[line] == Block::Paragraph
                    && !parsed.lines[line].is_empty()
                    && parsed.table_membership[line].is_none()
                    && parsed.code_membership[line].is_none()
            })
            .unwrap()
    });
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| model.move_to(edit_line, 0, false))
    });
    draw(cx);
    let before = cx.read(|app| editor.read(app).render_work());
    editor.update(cx, |editor, cx| editor.edit(cx, |model| model.insert("x")));
    draw(cx);
    let after = cx.read(|app| editor.read(app).render_work());
    assert_eq!(after.shaped_rows - before.shaped_rows, 1);
    assert_eq!(after.shaped_cells, before.shaped_cells);
    eprintln!(
        "long document: {lines} lines; settled={ready:?}; scroll_nodes={}, scroll_shapes={}; paragraph_shapes={}, paragraph_cells={}",
        scrolled.built_rows - stable.built_rows,
        scrolled.shaped_rows - stable.shaped_rows + scrolled.shaped_cells - stable.shaped_cells,
        after.shaped_rows - before.shaped_rows,
        after.shaped_cells - before.shaped_cells
    );
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn compact_separator_reduces_paragraph_spacing_without_changing_markdown(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let original = "# 标题\n\n## 小节\n\n1）第一段。\n\n2）第二段。\n\n3）第三段。";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value(original)
            .font_size(16.0)
            .style(MarkdownStyle {
                bare: true,
                compact_headings: true,
                compact_blank_lines: true,
                ..Default::default()
            });
        window.focus(&editor.focus);
        editor
    });
    cx.simulate_resize(size(px(1064.0), px(700.0)));
    draw(cx);
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        assert_eq!(editor.text(), original);
        assert_eq!(editor.layout.len(), 9);
        for line in [1, 3, 5, 7] {
            assert_eq!(editor.layout[line].height, 13.0);
        }
        for (previous, next) in [(4, 6), (6, 8)] {
            assert_eq!(editor.layout[previous].plan.kind, RowKind::Paragraph);
            assert_eq!(
                editor.layout[next].y.get() - editor.layout[previous].y.get(),
                39.0
            );
        }
    });
    editor.update(cx, |editor, cx| editor.set_font_size(20.0, cx));
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        assert_eq!(editor.layout[5].height, 16.0);
        assert_eq!(editor.layout[6].y.get() - editor.layout[4].y.get(), 48.0);
        assert_eq!(editor.text(), original);
    });
}

#[gpui::test]
fn compact_separator_preserves_explicit_blank_lines_code_and_source_mode(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let original = "\n开头\n\n段落\n\n\n末段\n\n```rust\n\n```\n";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value(original)
            .font_size(16.0)
            .style(MarkdownStyle {
                bare: true,
                compact_blank_lines: true,
                ..Default::default()
            });
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        for line in [0, 4, 5, 11] {
            assert_eq!(editor.layout[line].height, 26.0);
        }
        for line in [2, 7] {
            assert_eq!(editor.layout[line].height, 13.0);
        }
        assert!(matches!(editor.layout[9].plan.kind, RowKind::Code { .. }));
        assert_eq!(editor.layout[9].height, 22.0);
        assert_eq!(editor.text(), original);
    });
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        assert!(editor.layout.iter().all(|row| row.height == 22.0));
        assert_eq!(editor.text(), original);
    });
    editor.update(cx, |editor, cx| {
        editor.set_source_mode(false, cx);
        editor.set_text("", cx);
    });
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).layout[0].height, 26.0));
}

#[gpui::test]
fn compact_separator_updates_cached_neighbors_after_edit_undo_and_style_change(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let style = MarkdownStyle {
        bare: true,
        compact_blank_lines: true,
        ..Default::default()
    };
    let original = "first\n\nsecond";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value(original)
            .font_size(16.0)
            .style(style);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).layout[1].height, 13.0));
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| {
            model.move_to(2, 0, false);
            model.newline();
        });
    });
    draw(cx);
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        assert_eq!(editor.text(), "first\n\n\nsecond");
        assert_eq!(editor.layout[1].height, 26.0);
        assert_eq!(editor.layout[2].height, 26.0);
    });
    editor.update(cx, |editor, cx| editor.history(false, cx));
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        assert_eq!(editor.text(), original);
        assert_eq!(editor.layout[1].height, 13.0);
    });
    for compact in [false, true] {
        editor.update(cx, |editor, cx| {
            editor.set_style(
                MarkdownStyle {
                    compact_blank_lines: compact,
                    ..style
                },
                cx,
            )
        });
        draw(cx);
        cx.read(|app| {
            assert_eq!(
                editor.read(app).layout[1].height,
                if compact { 13.0 } else { 26.0 }
            );
        });
    }
}

#[gpui::test]
fn compact_separator_keeps_click_input_selection_and_caret_coordinates(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let original = "第一段。\n\n第二段。";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value(original)
            .font_size(16.0)
            .style(MarkdownStyle {
                bare: true,
                compact_blank_lines: true,
                ..Default::default()
            });
        window.focus(&editor.focus);
        editor
    });
    cx.update(|window, _| window.activate_window());
    draw(cx);
    draw(cx);
    let position = cx.read(|app| {
        let editor = editor.read(app);
        let row = &editor.layout[1];
        assert_eq!(row.height, 13.0);
        editor.text_bounds.origin + point(px(4.0), px(row.y.get() + row.height / 2.0))
    });
    cx.simulate_click(position, gpui::Modifiers::none());
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_eq!(editor.model.cursor(), Pos::new(1, 0));
            assert_painted_caret_at_cursor(editor, window, cx);
            editor.replace_and_mark_text_in_range(None, "pinyin", Some(6..6), window, cx);
        });
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_eq!(editor.layout[1].height, 26.0);
            assert_painted_caret_at_cursor(editor, window, cx);
            editor.replace_text_in_range(None, "新段🌱", window, cx);
        });
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_eq!(editor.text(), "第一段。\n新段🌱\n第二段。");
            assert_painted_caret_at_cursor(editor, window, cx);
            editor.history(false, cx);
        });
    });
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        assert_eq!(editor.text(), original);
        assert_eq!(editor.layout[1].height, 13.0);
    });
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| {
            model.move_to(0, 0, false);
            model.move_to(2, 4, true);
        });
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert_eq!(editor.model.copy().as_deref(), Some(original));
            assert_eq!(editor.layout[1].height, 13.0);
            assert_painted_caret_at_cursor(editor, window, cx);
        });
    });
}

#[gpui::test]
fn markdown_interaction_checkbox_keeps_the_scrolled_view(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = (0..80)
        .map(|i| {
            if i == 55 {
                "- [ ] 中文任务 🌱".into()
            } else {
                format!("paragraph {i}")
            }
        })
        .collect::<Vec<String>>()
        .join("\n");
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(&text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        window.focus(&editor.focus);
        editor
    });
    cx.simulate_resize(size(px(320.0), px(200.0)));
    draw(cx);
    draw(cx);
    for typewriter in [false, true] {
        editor.update(cx, |editor, cx| {
            editor.typewriter = typewriter;
            editor.scroll_request = None;
            editor
                .scroll
                .set_offset(point(px(0.0), px(-editor.layout[55].y.get() + 60.0)));
            cx.notify();
        });
        draw(cx);
        draw(cx);
        let (position, offset, cursor) = cx.read(|app| {
            let editor = editor.read(app);
            let row = &editor.layout[55];
            (
                editor.text_bounds.origin
                    + point(
                        px(row.inset / 2.0),
                        px(row.y.get() + row.pad_top + row.line_h / 2.0),
                    ),
                editor.scroll.offset(),
                editor.model.cursor(),
            )
        });
        assert!(position.y > px(0.0) && position.y < px(200.0));
        assert!(offset.y < px(-500.0));
        for checked in [true, false] {
            cx.simulate_click(position, gpui::Modifiers::none());
            draw(cx);
            cx.read(|app| {
                let editor = editor.read(app);
                assert_eq!(
                    editor.scroll.offset(),
                    offset,
                    "checkbox must not scroll to the old caret"
                );
                assert_eq!(editor.model.cursor(), cursor);
                assert_eq!(
                    editor.model.line(55),
                    Some(if checked {
                        "- [x] 中文任务 🌱"
                    } else {
                        "- [ ] 中文任务 🌱"
                    })
                );
            });
        }
    }
    editor.update(cx, |editor, cx| editor.history(false, cx));
    draw(cx);
    assert!(
        cx.read(|app| editor.read(app).text())
            .contains("- [x] 中文任务 🌱")
    );
    editor.update(cx, |editor, cx| editor.history(true, cx));
    draw(cx);
    assert_eq!(cx.read(|app| editor.read(app).text()), text);
}

#[gpui::test]
fn markdown_interaction_checkbox_preserves_selection_and_change_events(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value("中文 🌱\n\n- [ ] 待办");
        editor.model.move_to(0, 4, false);
        editor.model.move_to(0, 1, true);
        window.focus(&editor.focus);
        editor
    });
    let changes = Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = changes.clone();
    cx.update(|_, cx| {
        cx.subscribe(&editor, move |_, event, _| {
            if let MarkdownEditorEvent::Change(text) = event {
                observed.borrow_mut().push(text.clone());
            }
        })
        .detach();
    });
    draw(cx);
    draw(cx);
    let selection = cx.read(|app| editor.read(app).model.selection());
    editor.update(cx, |editor, cx| {
        assert!(editor.toggle_task(2, cx));
    });
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        assert_eq!(editor.model.selection(), selection);
        assert_eq!(editor.model.cursor(), Pos::new(0, 1));
    });
    assert_eq!(*changes.borrow(), ["中文 🌱\n\n- [x] 待办"]);
}

fn link_modifiers() -> gpui::Modifiers {
    gpui::Modifiers {
        platform: cfg!(target_os = "macos"),
        control: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}

#[gpui::test]
fn markdown_interaction_links_have_precise_regions_and_keep_editing(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = "intro\n\n[中文链接 🌱](https://example.com) 普通文字";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
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
    draw(cx);
    draw(cx);
    let bounds = cx
        .debug_bounds("markdown-link-2-0")
        .expect("link needs its own pointer region");
    cx.simulate_click(bounds.center(), link_modifiers());
    draw(cx);
    assert_eq!(*links.borrow(), ["https://example.com"]);
    assert_eq!(
        cx.read(|app| editor.read(app).model.cursor()),
        Pos::new(0, 0)
    );
    cx.simulate_click(
        point(bounds.right() + px(80.0), bounds.center().y),
        link_modifiers(),
    );
    assert_eq!(
        links.borrow().len(),
        1,
        "text beside a link must not open it"
    );
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    draw(cx);
    assert_eq!(
        links.borrow().len(),
        1,
        "plain click still edits in writable mode"
    );
    assert_eq!(cx.read(|app| editor.read(app).model.cursor().line), 2);
    assert_eq!(cx.read(|app| editor.read(app).text()), text);
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    // GPUI debug bounds retain old selectors across cached frames; verify
    // current event dispatch rather than interpreting stale bounds as elements.
    cx.simulate_click(bounds.center(), link_modifiers());
    draw(cx);
    assert_eq!(
        links.borrow().len(),
        1,
        "source mode must not activate preview links"
    );
    assert_eq!(cx.read(|app| editor.read(app).text()), text);
}

#[gpui::test]
fn markdown_interaction_wrapped_table_links_work_in_read_only_mode(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = "intro\n\n| 名称 | 说明 |\n| :---: | ---: |\n| 普通 | [很长的中文链接名称 🌱 English English English](https://example.com/table) |\n\n- [ ] 只读任务";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value(text)
            .read_only(true)
            .style(MarkdownStyle {
                bare: true,
                ..Default::default()
            });
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
    cx.simulate_resize(size(px(320.0), px(500.0)));
    draw(cx);
    draw(cx);
    let regions = [
        "markdown-link-4-0",
        "markdown-link-4-1",
        "markdown-link-4-2",
    ]
    .map(|selector| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing wrapped link region {selector}"))
    });
    for bounds in regions {
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        draw(cx);
    }
    assert_eq!(
        *links.borrow(),
        [
            "https://example.com/table",
            "https://example.com/table",
            "https://example.com/table"
        ]
    );
    assert!(
        cx.debug_bounds("markdown-task-6").is_none(),
        "read-only checkbox is not an action"
    );
    let position = cx.read(|app| {
        let editor = editor.read(app);
        let row = &editor.layout[6];
        editor.text_bounds.origin
            + point(
                px(row.inset / 2.0),
                px(row.y.get() + row.pad_top + row.line_h / 2.0),
            )
    });
    cx.simulate_click(position, gpui::Modifiers::none());
    draw(cx);
    assert_eq!(cx.read(|app| editor.read(app).text()), text);
}

#[gpui::test]
fn rapid_input_paints_the_caret_at_the_latest_insertion_point(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        window.focus(&editor.focus);
        editor
    });
    cx.update(|window, _| window.activate_window());
    cx.simulate_resize(size(px(300.0), px(200.0)));
    draw(cx);
    draw(cx);
    let mut expected = String::new();
    for batch in [
        "hello",
        " world",
        " 中文🌱",
        "\n",
        "next paragraph ",
        "fast typing ".repeat(20).as_str(),
    ] {
        // Deliver multiple native input callbacks before the next frame.
        for ch in batch.chars() {
            cx.update(|window, app| {
                editor.update(app, |editor, cx| {
                    editor.replace_text_in_range(None, &ch.to_string(), window, cx);
                })
            });
            expected.push(ch);
        }
        draw(cx);
        cx.update(|window, app| {
            editor.update(app, |editor, cx| {
                assert_eq!(editor.text(), expected);
                assert_eq!(
                    ime::selection(&editor.model),
                    expected.encode_utf16().count()..expected.encode_utf16().count()
                );
                assert_painted_caret_at_cursor(editor, window, cx);
            })
        });
    }
}

fn assert_painted_caret_at_cursor(
    editor: &mut MarkdownEditor,
    window: &mut Window,
    cx: &mut Context<MarkdownEditor>,
) {
    let painted = editor
        .caret_layer
        .read(cx)
        .painted_bounds
        .get()
        .expect("focused caret was painted");
    let offset = ime::offset_utf16(&editor.model, editor.model.cursor());
    let candidate = editor
        .bounds_for_range(offset..offset, Bounds::default(), window, cx)
        .unwrap();
    assert!(
        (f32::from(painted.origin.x - candidate.origin.x)).abs() < 1.0,
        "painted caret {painted:?} lags insertion point {candidate:?}"
    );
    assert!(
        (f32::from(painted.origin.y - candidate.origin.y)).abs() < 1.0,
        "painted caret {painted:?} lags insertion point {candidate:?}"
    );
}

#[gpui::test]
fn composition_and_source_mode_keep_the_painted_caret_with_the_text(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let original = format!("# 标题\n\n{}", "中文 English 🌱 ".repeat(25));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(&original);
        editor.model.doc_end(false);
        editor.scroll_request = Some(ScrollRequest::RevealCursor);
        window.focus(&editor.focus);
        editor
    });
    cx.update(|window, _| window.activate_window());
    cx.simulate_resize(size(px(310.0), px(200.0)));
    draw(cx);
    draw(cx);
    for source_mode in [false, true] {
        editor.update(cx, |editor, cx| editor.set_source_mode(source_mode, cx));
        draw(cx);
        for (preedit, selected) in [("p", 1..1), ("pinyin", 0..6), ("拼音", 1..1)] {
            cx.update(|window, app| {
                editor.update(app, |editor, cx| {
                    editor.replace_and_mark_text_in_range(
                        None,
                        preedit,
                        Some(selected),
                        window,
                        cx,
                    );
                })
            });
            draw(cx);
            cx.update(|window, app| {
                editor.update(app, |editor, cx| {
                    assert!(editor.is_composing());
                    assert_painted_caret_at_cursor(editor, window, cx);
                })
            });
        }
        cx.update(|window, app| {
            editor.update(app, |editor, cx| {
                editor.replace_text_in_range(None, "输入法", window, cx);
                editor.replace_text_in_range(None, " fast", window, cx);
            })
        });
        draw(cx);
        cx.update(|window, app| {
            editor.update(app, |editor, cx| {
                assert_eq!(editor.text(), format!("{original}输入法 fast"));
                assert_painted_caret_at_cursor(editor, window, cx);
                editor.history(false, cx);
            })
        });
        draw(cx);
        assert_eq!(
            cx.read(|app| editor.read(app).text()),
            format!("{original}输入法")
        );
        editor.update(cx, |editor, cx| editor.history(false, cx));
        draw(cx);
        cx.update(|window, app| {
            editor.update(app, |editor, cx| {
                assert_eq!(editor.text(), original);
                assert_painted_caret_at_cursor(editor, window, cx);
            })
        });
    }
}

#[gpui::test]
fn focus_and_cross_cell_selection_update_syntax_reveal_in_cached_rows(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value("**bold**\n\n| **a** | **b** |\n| --- | --- |\n| c | d |")
            .style(MarkdownStyle {
                bare: true,
                ..Default::default()
            });
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    assert_eq!(
        cx.read(|app| editor.read(app).layout[0].plan.visible.clone()),
        "**bold**"
    );
    cx.update(|window, _| window.blur());
    draw(cx);
    assert_eq!(
        cx.read(|app| editor.read(app).layout[0].plan.visible.clone()),
        "bold"
    );
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            window.focus(&editor.focus);
            editor.edit(cx, |model| model.move_to(2, 2, false));
        })
    });
    draw(cx);
    assert_eq!(
        cx.read(|app| editor.read(app).layout[2].cells[1].row.plan.visible.clone()),
        "b"
    );
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| {
            model.move_to(2, 2, false);
            model.move_to(2, 6, true);
        })
    });
    draw(cx);
    assert_eq!(
        cx.read(|app| editor.read(app).layout[2].cells[1].row.plan.visible.clone()),
        "**b**"
    );
}

#[gpui::test]
fn scrolling_reuses_glyphs_and_keeps_caret_and_ime_coordinates_aligned(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = (0..400)
        .map(|i| format!("中文 paragraph {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(&text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    let before = cx.read(|app| editor.read(app).render_work());
    cx.simulate_mouse_move(point(px(100.0), px(100.0)), None, gpui::Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(100.0), px(100.0)),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-600.0))),
        ..Default::default()
    });
    draw(cx);
    let after = cx.read(|app| editor.read(app).render_work());
    assert!(after.document_renders > before.document_renders);
    assert_eq!(after.shaped_rows, before.shaped_rows);
    assert_eq!(after.parsed_documents, before.parsed_documents);
    assert!(after.built_rows - before.built_rows < 150);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            assert!(editor.scroll.offset().y < px(0.0));
            let caret = editor.caret_layer.read(cx).bounds.unwrap();
            let candidate = editor
                .bounds_for_range(0..0, Bounds::default(), window, cx)
                .unwrap();
            let body_origin = editor.scroll.bounds().origin;
            assert!((f32::from(candidate.origin.x - body_origin.x - caret.origin.x)).abs() < 1.0);
            assert!((f32::from(candidate.origin.y - body_origin.y - caret.origin.y)).abs() < 1.0);
            assert!(!editor.caret_is_visible(cx));
        })
    });
}

#[gpui::test]
fn cached_input_handler_still_commits_composition_and_undo_after_caret_redraw(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx)
            .value("中文 🌱")
            .style(MarkdownStyle {
                bare: true,
                ..Default::default()
            });
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    editor.update(cx, |editor, cx| {
        editor.caret_layer.update(cx, |layer, cx| {
            layer.visible = !layer.visible;
            cx.notify();
        })
    });
    draw(cx);
    cx.simulate_input("prefix ");
    draw(cx);
    assert_eq!(cx.read(|app| editor.read(app).text()), "prefix 中文 🌱");
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "拼音", Some(2..2), window, cx);
        })
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            let bounds = editor
                .bounds_for_range(7..9, Bounds::default(), window, cx)
                .unwrap();
            assert!(bounds.size.height > px(0.0));
            assert!(editor.is_composing());
            editor.replace_text_in_range(None, "输入", window, cx);
        })
    });
    draw(cx);
    assert_eq!(cx.read(|app| editor.read(app).text()), "prefix 输入中文 🌱");
    cx.dispatch_action(actions::Undo);
    draw(cx);
    assert_eq!(cx.read(|app| editor.read(app).text()), "prefix 中文 🌱");
}

#[gpui::test]
fn resize_font_and_theme_invalidate_layout_without_losing_content(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = "中文长段落 ".repeat(100);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(&text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    let first = cx.read(|app| editor.read(app).render_work());
    cx.simulate_resize(size(px(250.0), px(500.0)));
    draw(cx);
    draw(cx);
    let narrowed = cx.read(|app| editor.read(app).render_work());
    assert!(narrowed.shaped_rows > first.shaped_rows);
    editor.update(cx, |editor, cx| editor.set_font_size(20.0, cx));
    draw(cx);
    let resized = cx.read(|app| editor.read(app).render_work());
    assert!(resized.shaped_rows > narrowed.shaped_rows);
    cx.update(|_, app| {
        guise::Theme::dark().init(app);
        editor.update(app, |editor, cx| {
            editor.set_style(
                MarkdownStyle {
                    bare: true,
                    ..Default::default()
                },
                cx,
            )
        });
    });
    draw(cx);
    let themed = cx.read(|app| editor.read(app).render_work());
    assert!(themed.shaped_rows > resized.shaped_rows);
    assert_eq!(themed.parsed_documents, first.parsed_documents);
    assert_eq!(cx.read(|app| editor.read(app).text()), text);
}

#[gpui::test]
fn source_roundtrip_retains_mermaid_image_zoom_and_highlight_cache(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let key = DiagramKey {
        source: "flowchart LR\nA --> B".into(),
        dark: false,
    };
    let diagram = diagrams::render(&key).unwrap();
    let image = diagram.image.clone();
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value("paragraph\n\n```mermaid\nflowchart LR\nA --> B\n```\n\n```rust\nlet number = 42;\n```").style(MarkdownStyle {bare: true, ..Default::default()});
        window.focus(&editor.focus); editor
    });
    draw(cx);
    draw(cx);
    editor.update(cx, |editor, cx| {
        editor
            .diagrams
            .insert(key.clone(), DiagramState::Ready(diagram));
        editor
            .diagram_views
            .get_mut(&2)
            .unwrap()
            .set_zoom(Some(1.25));
        cx.notify();
    });
    draw(cx);
    let tokens = cx.read(|app| editor.read(app).code_highlights.line(7, 0).to_vec());
    assert!(!tokens.is_empty());
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    editor.update(cx, |editor, cx| editor.set_source_mode(false, cx));
    draw(cx);
    cx.read(|app| {
        let editor = editor.read(app);
        let DiagramState::Ready(diagram) = &editor.diagrams[&key] else {
            panic!("preview was regenerated")
        };
        assert!(std::sync::Arc::ptr_eq(&image, &diagram.image));
        assert_eq!(editor.diagram_views[&2].zoom, Some(1.25));
        assert_eq!(editor.code_highlights.line(7, 0), tokens);
    });
}

#[gpui::test]
fn caret_updates_do_not_reparse_relayout_or_rebuild_the_cached_document(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = (0..400)
        .map(|i| format!("paragraph {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(&text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    let before = cx.read(|app| editor.read(app).render_work());
    assert!(before.shaped_rows > 0);
    assert!(before.built_rows < before.document_renders * 400);
    editor.update(cx, |editor, cx| {
        editor.caret.visible = !editor.caret.visible;
        editor.caret_layer.update(cx, |layer, cx| {
            layer.visible = !layer.visible;
            cx.notify();
        });
    });
    draw(cx);
    let after = cx.read(|app| editor.read(app).render_work());
    assert_eq!(after.document_renders, before.document_renders);
    assert_eq!(after.parsed_documents, before.parsed_documents);
    assert_eq!(after.shaped_rows, before.shaped_rows);
    assert_eq!(after.built_rows, before.built_rows);
}

#[gpui::test]
fn returning_from_source_reuses_rich_layout_and_one_cell_edit_does_not_relayout_other_rows(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let text = "outside\n\n| a | b |\n| --- | --- |\n| c | d |\n| e | f |\n\nafter";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let editor = MarkdownEditor::new(cx).value(text).style(MarkdownStyle {
            bare: true,
            ..Default::default()
        });
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    draw(cx);
    editor.update(cx, |editor, cx| editor.set_source_mode(true, cx));
    draw(cx);
    let source = cx.read(|app| editor.read(app).render_work());
    editor.update(cx, |editor, cx| editor.set_source_mode(false, cx));
    draw(cx);
    let rich = cx.read(|app| editor.read(app).render_work());
    assert_eq!(rich.parsed_documents, source.parsed_documents);
    assert_eq!(rich.shaped_rows, source.shaped_rows);
    assert_eq!(rich.shaped_cells, source.shaped_cells);
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| model.move_to(4, 2, false))
    });
    draw(cx);
    let before = cx.read(|app| editor.read(app).render_work());
    editor.update(cx, |editor, cx| {
        editor.edit(cx, |model| {
            model.move_to(4, 2, false);
            model.move_to(4, 3, true);
            model.insert("x");
        })
    });
    draw(cx);
    let after = cx.read(|app| editor.read(app).render_work());
    assert_eq!(after.shaped_cells - before.shaped_cells, 2);
    assert_eq!(after.shaped_rows, before.shaped_rows);
    assert!(cx.read(|app| editor.read(app).text()).contains("| x | d |"));
}
