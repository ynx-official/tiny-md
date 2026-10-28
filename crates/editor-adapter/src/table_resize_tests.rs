use super::*;

const SOURCE: &str =
    "前文\n\n| 名称 | 数值 |\n| :---: | ---: |\n| 中文🌱 | `a\\|b` |\n| 缺失 |\n\n后文";

#[test]
fn table_resize_grows_bottom_and_right_preserving_cells_alignment_and_history() {
    let mut model = EditorModel::new(SOURCE);
    focus_cell(&mut model, 4, 1);
    assert_eq!(resize_loss(&model, 4, 5), Some(0));
    assert!(edit(
        &mut model,
        TableCommand::Resize {
            columns: 4,
            rows: 5
        }
    ));
    let table = &collect(model.lines()).0[0];
    assert_eq!(table.rows.len(), 6);
    assert_eq!(
        table.alignments,
        [
            Alignment::Center,
            Alignment::Right,
            Alignment::Left,
            Alignment::Left
        ]
    );
    assert!(model.text().contains("| 中文🌱 | `a\\|b` |  |  |"));
    assert!(model.text().starts_with("前文\n\n"));
    assert!(model.text().ends_with("\n\n后文"));
    assert_eq!(model.cursor().line, 4);
    let changed = model.text();
    assert!(model.undo());
    assert_eq!(model.text(), SOURCE);
    assert!(model.redo());
    assert_eq!(model.text(), changed);
}

#[test]
fn table_resize_counts_deleted_content_and_clamps_cursor_to_surviving_cell() {
    let mut model = EditorModel::new(SOURCE);
    focus_cell(&mut model, 5, 0);
    assert_eq!(resize_loss(&model, 1, 2), Some(3));
    assert!(edit(
        &mut model,
        TableCommand::Resize {
            columns: 1,
            rows: 2
        }
    ));
    assert_eq!(
        model.text(),
        "前文\n\n| 名称 |\n| :---: |\n| 中文🌱 |\n\n后文"
    );
    assert_eq!(model.cursor().line, 4);
    assert!(model.undo());
    assert_eq!(model.text(), SOURCE);
}

#[test]
fn table_resize_noop_and_invalid_sizes_keep_exact_source_and_history() {
    for (columns, rows) in [
        (2, 3),
        (0, 3),
        (2, 0),
        (usize::MAX, 2),
        (2, usize::MAX),
        (128, 2048),
    ] {
        let mut model = EditorModel::new(SOURCE);
        model.move_to(4, 3, false);
        let cursor = model.cursor();
        assert!(!edit(&mut model, TableCommand::Resize { columns, rows }));
        assert_eq!(model.text(), SOURCE);
        assert_eq!(model.cursor(), cursor);
        assert!(!model.undo());
    }
}

#[test]
fn table_resize_keeps_header_and_retains_or_reports_overflow_source() {
    let source = "| A | B |\n| --- | --- |\n| 甲 | 乙 | 隐藏🌱 |\n| | |";
    let mut model = EditorModel::new(source);
    model.move_to(2, 3, false);
    assert_eq!(resize_loss(&model, 2, 2), Some(0));
    assert!(edit(
        &mut model,
        TableCommand::Resize {
            columns: 2,
            rows: 2
        }
    ));
    assert!(model.text().contains("隐藏🌱"));
    assert_eq!(resize_loss(&model, 1, 2), Some(3));
    assert!(edit(
        &mut model,
        TableCommand::Resize {
            columns: 3,
            rows: 2
        }
    ));
    assert!(model.text().contains("| 甲 | 乙 | 隐藏🌱 |"));
    assert_eq!(resize_loss(&model, 3, 1), Some(3));
    assert!(edit(
        &mut model,
        TableCommand::Resize {
            columns: 3,
            rows: 1
        }
    ));
    assert_eq!(collect(model.lines()).0[0].rows.len(), 2);
    assert!(model.text().starts_with("| A | B |  |"));
}
