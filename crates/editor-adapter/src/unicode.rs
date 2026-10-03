//! User-perceived characters, including combining marks and emoji sequences.
use crate::ime::{byte_at_utf16, offset_utf16, pos_byte};
use guise::editor::EditorModel;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn move_grapheme(model: &mut EditorModel, right: bool, extend: bool) {
    if !extend && let Some((start, end)) = model.selection() {
        let pos = if right { end } else { start };
        model.move_to(pos.line, pos.col, false);
        return;
    }
    let source = model.text();
    let byte = byte_at_utf16(&source, offset_utf16(model, model.cursor()), false);
    let next = if right {
        source
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|&i| i > byte)
            .unwrap_or(source.len())
    } else {
        source
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|&i| i < byte)
            .last()
            .unwrap_or(0)
    };
    let pos = pos_byte(&source, next);
    model.move_to(pos.line, pos.col, extend);
}

pub(crate) fn delete_grapheme(model: &mut EditorModel, right: bool) -> bool {
    if model.selection().is_none() {
        move_grapheme(model, right, true);
    }
    model.delete_selection()
}

#[cfg(test)]
mod tests {
    use super::*;
    use guise::editor::Pos;

    #[test]
    fn deletion_keeps_emoji_sequences_and_combining_marks_whole() {
        let mut model = EditorModel::new("中👩‍💻e\u{301}");
        model.doc_end(false);
        assert!(delete_grapheme(&mut model, false));
        assert_eq!(model.text(), "中👩‍💻");
        assert!(delete_grapheme(&mut model, false));
        assert_eq!(model.text(), "中");
        assert!(model.undo());
        assert_eq!(model.text(), "中👩‍💻");
    }

    #[test]
    fn arrows_cross_lines_and_collapse_selection() {
        let mut model = EditorModel::new("🌱\n中文");
        move_grapheme(&mut model, true, false);
        assert_eq!(model.cursor(), Pos::new(0, 1));
        move_grapheme(&mut model, true, true);
        assert_eq!(model.cursor(), Pos::new(1, 0));
        move_grapheme(&mut model, false, false);
        assert_eq!(model.cursor(), Pos::new(0, 1));
        assert!(model.selection().is_none());
    }
}
