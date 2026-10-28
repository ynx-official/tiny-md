use super::*;
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext, size};

const SOURCE: &str =
    "前文\n\n| 姓名 | 内容 |\n| --- | ---: |\n| 中文🌱 | `a\\|b` |\n| 第二行 | **粗体** |\n\n后文";

#[gpui::test]
fn table_size_button_opens_without_changing_source_selection_or_layout(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        editor.model.move_to(4, 5, false);
        editor.model.move_to(4, 2, true);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    let before = cx.read(|app| {
        let e = editor.read(app);
        (
            e.model.cursor(),
            e.model.selection(),
            e.layout[7].y.get(),
            e.scroll.offset(),
        )
    });
    click(cx, "table-size-button");
    assert!(cx.debug_bounds("table-size-panel").is_some());
    cx.read(|app| {
        let e = editor.read(app);
        assert_eq!(e.text(), SOURCE);
        assert_eq!(
            (
                e.model.cursor(),
                e.model.selection(),
                e.layout[7].y.get(),
                e.scroll.offset()
            ),
            before
        );
    });
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |e, _| {
            assert!(e.focus.is_focused(window));
            assert_eq!(e.model.selection(), before.1);
            assert!(!e.model.undo());
        })
    });
}

#[gpui::test]
fn table_size_grid_changes_dimensions_in_one_undo_step(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::dark().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        tables::focus_cell(&mut editor.model, 4, 1);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    click(cx, "table-size-button");
    let grid_cell = cx.debug_bounds("table-size-cell-4-5").unwrap().center();
    cx.simulate_mouse_move(grid_cell, None, gpui::Modifiers::none());
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
    click(cx, "table-size-cell-4-5");
    cx.read(|app| {
        let e = editor.read(app);
        let table = &tables::collect(e.model.lines()).0[0];
        assert_eq!((table.alignments.len(), table.rows.len()), (4, 6));
        assert_eq!(table.alignments[1], Alignment::Right);
        assert!(e.text().contains("`a\\|b`"));
        assert!(!e.table_menu_open(app));
    });
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
}

#[gpui::test]
fn table_size_shrink_confirms_content_loss_and_blank_shrink_applies_directly(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        tables::focus_cell(&mut editor.model, 5, 1);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    click(cx, "table-size-button");
    click(cx, "table-size-cell-1-1");
    cx.read(|app| {
        assert_eq!(editor.read(app).text(), SOURCE);
        assert!(editor.read(app).table_menu_open(app));
    });
    // Cancel the pending deletion without adding a history item.
    click(cx, "table-size-cancel");
    editor.update(cx, |e, _| assert!(!e.model.undo()));
    click(cx, "table-size-button");
    click(cx, "table-size-cell-1-1");
    cx.simulate_keystrokes("enter");
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), "前文\n\n| 姓名 |\n| --- |\n\n后文"));
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
    click(cx, "table-size-button");
    click(cx, "table-size-cell-4-5");
    click(cx, "table-size-button");
    click(cx, "table-size-cell-2-3");
    cx.read(|app| {
        assert_eq!(editor.read(app).text(), SOURCE);
        assert!(!editor.read(app).table_menu_open(app));
    });
}

#[gpui::test]
fn table_size_numeric_keyboard_validates_and_applies_beyond_grid(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        tables::focus_cell(&mut editor.model, 4, 1);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    cx.simulate_keystrokes("shift-f10 end up enter");
    draw(cx);
    assert!(cx.debug_bounds("table-size-panel").is_some());
    cx.simulate_keystrokes("tab");
    draw(cx);
    cx.simulate_input("0");
    cx.simulate_keystrokes("enter");
    draw(cx);
    cx.read(|app| {
        assert_eq!(editor.read(app).text(), SOURCE);
        assert!(editor.read(app).table_menu_open(app));
    });
    cx.simulate_keystrokes("shift-tab tab");
    draw(cx);
    cx.simulate_input("8");
    cx.simulate_keystrokes("tab");
    draw(cx);
    cx.simulate_input("12");
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
    cx.simulate_keystrokes("enter");
    draw(cx);
    cx.read(|app| {
        let e = editor.read(app);
        let table = &tables::collect(e.model.lines()).0[0];
        assert_eq!((table.alignments.len(), table.rows.len()), (8, 13));
        assert!(!e.table_menu_open(app));
    });
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
}

#[gpui::test]
fn table_size_fits_small_window_and_discards_stale_input(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::dark().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        tables::focus_cell(&mut editor.model, 4, 1);
        window.focus(&editor.focus);
        editor
    });
    cx.simulate_resize(size(px(320.0), px(240.0)));
    draw(cx);
    cx.simulate_keystrokes("shift-f10 end up enter");
    draw(cx);
    let panel = cx.debug_bounds("table-size-panel").unwrap();
    assert!(panel.left() >= px(8.0) && panel.right() <= px(312.0));
    assert!(panel.top() >= px(8.0) && panel.bottom() <= px(232.0));
    cx.simulate_keystrokes("tab");
    cx.simulate_input("9");
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|window, app| {
        assert!(editor.read(app).focus.is_focused(window));
        assert_eq!(editor.read(app).text(), SOURCE);
    });
    cx.simulate_keystrokes("shift-f10 end up enter tab");
    draw(cx);
    editor.update(cx, |e, cx| {
        e.apply_external_text(&SOURCE.replace("姓名", "更新"), cx)
    });
    draw(cx);
    cx.read(|app| assert!(!editor.read(app).table_menu_open(app)));
    cx.update(|window, app| assert!(editor.read(app).focus.is_focused(window)));
}

#[gpui::test]
fn table_controls_exclude_markdown_looking_text_inside_html_tables(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "<table>\n<tr><td>\n| A | B |\n| --- | --- |\n| x | y |\n</td></tr>\n</table>";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(source);
        editor.model.move_to(4, 3, false);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    assert!(cx.debug_bounds("table-toolbar").is_none());
    cx.simulate_keystrokes("shift-f10");
    draw(cx);
    cx.read(|app| assert!(!editor.read(app).table_menu_open(app)));
    editor.update(cx, |e, cx| {
        e.command(
            crate::EditorCommand::TableEdit(crate::TableCommand::DeleteTable),
            cx,
        )
    });
    cx.read(|app| assert_eq!(editor.read(app).text(), source));
}

fn draw(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    let bounds = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("missing {id}"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    draw(cx);
}

#[gpui::test]
fn table_menu_copy_and_escape_preserve_reverse_selection_layout_and_history(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        editor.model.move_to(4, 5, false);
        editor.model.move_to(4, 2, true);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    let before = cx.read(|app| {
        let e = editor.read(app);
        (
            e.model.cursor(),
            e.model.selection(),
            e.layout[7].y.get(),
            e.scroll.offset(),
        )
    });
    click(cx, "table-more");
    assert!(cx.debug_bounds("table-toolbar").is_some());
    assert!(cx.debug_bounds("table-menu").is_some());
    cx.read(|app| {
        let e = editor.read(app);
        assert_eq!(
            (
                e.model.cursor(),
                e.model.selection(),
                e.layout[7].y.get(),
                e.scroll.offset()
            ),
            before
        );
        assert_eq!(e.text(), SOURCE);
    });
    click(cx, "table-copy");
    cx.read(|app| {
        assert!(
            !editor.read(app).table_menu_open(app),
            "copy should close menu state"
        )
    });
    assert_eq!(
        cx.read_from_clipboard().unwrap().text().unwrap(),
        SOURCE
            .lines()
            .skip(2)
            .take(4)
            .collect::<Vec<_>>()
            .join("\n")
    );
    // GPUI 0.2.2 retains debug_bounds entries after elements disappear. Verify
    // dismissal through focus and subsequent input, not stale hitbox metadata.
    cx.simulate_keystrokes("shift-f10 escape");
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |e, _| {
            assert!(e.focus.is_focused(window));
            assert_eq!(
                (
                    e.model.cursor(),
                    e.model.selection(),
                    e.layout[7].y.get(),
                    e.scroll.offset()
                ),
                before
            );
            assert!(!e.model.undo());
            assert_eq!(e.text(), SOURCE);
        })
    });
    cx.simulate_input("替");
    draw(cx);
    cx.read(|app| assert!(editor.read(app).text().contains("替")));
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
}

#[gpui::test]
fn table_menu_edits_original_column_and_toolbar_alignment_undo_as_one_step(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| guise::Theme::dark().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        tables::focus_cell(&mut editor.model, 4, 1);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    click(cx, "table-more");
    click(cx, "table-move-column-left");
    cx.read(|app| {
        let e = editor.read(app);
        assert!(e.text().contains("| `a\\|b` | 中文🌱 |"));
        assert_eq!(e.model.cursor().line, 4);
        assert_eq!(
            tables::collect(e.model.lines()).0[0].alignments,
            [Alignment::Right, Alignment::Left]
        );
    });
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
    editor.update(cx, |e, cx| {
        tables::focus_cell(&mut e.model, 4, 1);
        cx.notify();
    });
    draw(cx);
    click(cx, "table-align-center");
    cx.read(|app| {
        assert_eq!(
            tables::collect(editor.read(app).model.lines()).0[0].alignments[1],
            Alignment::Center
        )
    });
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
}

#[gpui::test]
fn table_menu_keyboard_skips_disabled_items_and_fits_small_window(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let source = "| 标题 |\n| --- |\n| 内容 |";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(source);
        tables::focus_cell(&mut editor.model, 0, 0);
        window.focus(&editor.focus);
        editor
    });
    cx.simulate_resize(size(px(320.0), px(220.0)));
    draw(cx);
    cx.simulate_keystrokes("shift-f10");
    draw(cx);
    let menu = cx.debug_bounds("table-menu").expect("keyboard menu");
    assert!(menu.left() >= px(0.0) && menu.right() <= px(320.0));
    assert!(menu.top() >= px(0.0) && menu.bottom() <= px(220.0));
    // Header cannot be moved/deleted; Home skips the disabled 'row above'.
    cx.simulate_keystrokes("end home enter");
    draw(cx);
    cx.read(|app| {
        let e = editor.read(app);
        assert_eq!(tables::collect(e.model.lines()).0[0].rows.len(), 4);
        assert_eq!(e.model.cursor().line, 2);
    });
    cx.dispatch_action(actions::Undo);
    draw(cx);
    editor.update(cx, |e, cx| {
        tables::focus_cell(&mut e.model, 0, 0);
        cx.notify();
    });
    draw(cx);
    cx.simulate_keystrokes("shift-f10 tab enter");
    draw(cx);
    cx.read(|app| {
        assert_eq!(
            tables::collect(editor.read(app).model.lines()).0[0]
                .alignments
                .len(),
            2
        )
    });
}

#[gpui::test]
fn table_toolbar_hides_for_prose_source_readonly_html_and_stale_menu(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        tables::focus_cell(&mut editor.model, 4, 1);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    click(cx, "table-more");
    editor.update(cx, |e, cx| {
        e.apply_external_text(&SOURCE.replace("中文🌱", "外部修改"), cx);
    });
    draw(cx);
    cx.read(|app| {
        assert!(
            !editor.read(app).table_menu_open(app),
            "changed document should close menu state"
        )
    });
    let old_delete = cx.debug_bounds("table-delete-button").unwrap().center();
    editor.update(cx, |e, cx| e.set_source_mode(true, cx));
    draw(cx);
    let text = cx.read(|app| editor.read(app).text());
    cx.simulate_click(old_delete, gpui::Modifiers::none());
    draw(cx);
    cx.read(|app| {
        assert_eq!(
            editor.read(app).text(),
            text,
            "source mode must remove the delete control"
        )
    });
    editor.update(cx, |e, cx| {
        e.set_source_mode(false, cx);
        e.set_read_only(true, cx);
    });
    draw(cx);
    cx.simulate_click(old_delete, gpui::Modifiers::none());
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), text));
    cx.simulate_keystrokes("shift-f10");
    draw(cx);
    cx.read(|app| assert!(!editor.read(app).table_menu_open(app)));
    editor.update(cx, |e, cx| {
        e.set_read_only(false, cx);
        e.model.move_to(0, 0, false);
        e.model.move_to(4, 3, true);
        cx.notify();
    });
    draw(cx);
    cx.simulate_click(old_delete, gpui::Modifiers::none());
    draw(cx);
    cx.read(|app| {
        assert_eq!(
            editor.read(app).text(),
            text,
            "cross-table selection must not expose a delete action"
        )
    });
    editor.update(cx, |e, cx| {
        e.set_text("<table>\n<tr><td>中文</td></tr>\n</table>", cx)
    });
    draw(cx);
    cx.simulate_keystrokes("shift-f10");
    draw(cx);
    cx.read(|app| assert!(!editor.read(app).table_menu_open(app)));
}

#[gpui::test]
fn table_toolbar_commits_composition_and_keeps_deletion_undo_separate(cx: &mut TestAppContext) {
    cx.update(|cx| guise::Theme::light().init(cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::new(cx).value(SOURCE);
        tables::focus_cell(&mut editor.model, 4, 0);
        window.focus(&editor.focus);
        editor
    });
    draw(cx);
    cx.update(|window, app| {
        editor.update(app, |e, cx| {
            e.replace_and_mark_text_in_range(None, "输入", Some(2..2), window, cx);
        })
    });
    draw(cx);
    click(cx, "table-delete-button");
    cx.read(|app| assert_eq!(editor.read(app).text(), "前文\n\n\n\n后文"));
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| {
        assert_eq!(
            editor.read(app).text(),
            SOURCE.replace("中文🌱", "输入中文🌱")
        )
    });
    cx.dispatch_action(actions::Undo);
    draw(cx);
    cx.read(|app| assert_eq!(editor.read(app).text(), SOURCE));
}
