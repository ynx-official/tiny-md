use super::*;
use gpui::TestAppContext;
use gpui::{EntityInputHandler, ScrollDelta, ScrollWheelEvent, size};

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
        editor.scroll_to_cursor = true;
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
