use guise::editor::{EditorModel, Pos};

/// Synchronization is one undo step and never resets the previous history.
pub(crate) fn apply(model: &mut EditorModel, target: &str) {
    let source = model.text();
    if source == target {
        return;
    }
    let cursor = model.cursor();
    let selection = model.selection();
    let anchor = selection.map(|(start, end)| if cursor == start { end } else { start });
    let mut options = diffy::DiffOptions::new();
    options.set_context_len(0);
    let patch = options.create_patch(&source, target);
    let cursor = map_position(&patch, cursor);
    let anchor = anchor.map(|pos| map_position(&patch, pos));
    crate::ime::apply_difference(model, target);
    if let Some(anchor) = anchor {
        model.move_to(anchor.line, anchor.col, false);
        model.move_to(cursor.line, cursor.col, true);
    } else {
        model.move_to(cursor.line, cursor.col, false);
    }
}

fn map_position(patch: &diffy::Patch<'_, str>, pos: Pos) -> Pos {
    let mut delta = 0isize;
    for hunk in patch.hunks() {
        let old = hunk.old_range();
        let new = hunk.new_range();
        let old_start = old.start() - usize::from(!old.is_empty());
        let new_start = new.start() - usize::from(!new.is_empty());
        if pos.line < old_start {
            break;
        }
        if pos.line < old_start + old.len() {
            return if new.is_empty() {
                Pos::new(new_start, 0)
            } else {
                Pos::new(
                    new_start + (pos.line - old_start).min(new.len() - 1),
                    pos.col,
                )
            };
        }
        delta = (new_start + new.len()) as isize - (old_start + old.len()) as isize;
    }
    Pos::new(pos.line.saturating_add_signed(delta), pos.col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_preserves_cursor_in_unchanged_text_unicode_and_earlier_undo_history() {
        let mut model = EditorModel::new("前文\n中文 🌱\n后文");
        model.move_to(1, 4, false);
        model.insert("A");
        apply(&mut model, "新增\n前文\n中文 🌱A\n后文\n外部");
        assert_eq!(model.cursor(), Pos::new(2, 5));
        assert!(model.undo());
        assert_eq!(model.text(), "前文\n中文 🌱A\n后文");
        assert!(model.undo());
        assert_eq!(model.text(), "前文\n中文 🌱\n后文");
    }

    #[test]
    fn sync_preserves_a_reversed_selection_and_noop_adds_no_history() {
        let mut model = EditorModel::new("before\nselect 🌱\nafter");
        model.move_to(1, 8, false);
        model.move_to(1, 0, true);
        apply(&mut model, "added\nbefore\nselect 🌱\nafter");
        assert_eq!(model.cursor(), Pos::new(2, 0));
        assert_eq!(model.selected_text().as_deref(), Some("select 🌱"));
        let unchanged = model.text();
        apply(&mut model, &unchanged);
        assert!(model.undo());
        assert_eq!(model.text(), "before\nselect 🌱\nafter");
        assert!(!model.undo());
    }

    #[test]
    fn deleted_cursor_lines_and_shorter_replacements_clamp_to_valid_positions() {
        let mut model = EditorModel::new("keep\n删除 🌱\nend");
        model.move_to(1, 4, false);
        apply(&mut model, "keep\nend");
        assert_eq!(model.cursor(), Pos::new(1, 0));
        model.move_to(1, 3, false);
        apply(&mut model, "keep\n短");
        assert_eq!(model.cursor(), Pos::new(1, 1));
        apply(&mut model, "");
        assert_eq!(model.cursor(), Pos::new(0, 0));
        assert!(model.undo());
        assert_eq!(model.text(), "keep\n短");
    }
}
