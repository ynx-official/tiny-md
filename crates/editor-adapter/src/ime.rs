//! Platform UTF-16 input over Guise's char-indexed model.
//!
//! Preedit updates are provisional. On commit, apply one replacement to the
//! pre-composition model so intermediate candidates never enter undo history.
use guise::editor::{EditorModel, Pos};
use std::ops::Range;

#[derive(Default)]
pub(crate) struct ImeState {
    baseline: Option<EditorModel>,
    marked: Option<Range<usize>>,
}

impl ImeState {
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }
    pub fn active(&self) -> bool {
        self.baseline.is_some()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update(
        &mut self,
        model: &mut EditorModel,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) {
        if text.is_empty() {
            if let Some(baseline) = self.baseline.take() {
                *model = baseline;
            }
            self.marked = None;
            return;
        }
        if self.baseline.is_none() {
            self.baseline = Some(model.clone());
        }
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| selection(model));
        let source = model.text();
        let bytes = byte_range(&source, range);
        let start = source[..bytes.start].encode_utf16().count();
        let text = normalize(text);
        replace(model, &source, bytes, &text);
        let len = text.encode_utf16().count();
        self.marked = Some(start..start + len);
        let selected = selected.unwrap_or(len..len);
        let s = start + selected.start.min(len);
        let e = start + selected.end.min(len).max(selected.start.min(len));
        select_utf16(model, s..e);
    }

    pub fn commit(&mut self, model: &mut EditorModel, range: Option<Range<usize>>, text: &str) {
        // An empty replacement of the marked region is cancellation. Restore
        // the original selection as well, rather than deleting selected prose.
        if text.is_empty() && self.baseline.is_some() && (range.is_none() || range == self.marked) {
            *model = self.baseline.take().expect("composition baseline");
            self.marked = None;
            return;
        }
        let source = model.text();
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| selection(model));
        let bytes = byte_range(&source, range);
        let text = normalize(text);
        let cursor = source[..bytes.start].encode_utf16().count() + text.encode_utf16().count();
        let target = format!("{}{}{}", &source[..bytes.start], text, &source[bytes.end..]);
        if let Some(baseline) = self.baseline.take() {
            *model = baseline;
            apply_difference(model, &target);
        } else if source != target {
            replace(model, &source, bytes, &text);
        }
        self.marked = None;
        if selection(model) != (cursor..cursor) {
            select_utf16(model, cursor..cursor);
        }
    }

    /// Accept the currently marked text, as required by the platform contract.
    pub fn unmark(&mut self, model: &mut EditorModel) {
        let target = model.text();
        let selected = selection(model);
        if let Some(baseline) = self.baseline.take() {
            *model = baseline;
            apply_difference(model, &target);
            select_utf16(model, selected);
        }
        self.marked = None;
    }
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn selection(model: &EditorModel) -> Range<usize> {
    let (start, end) = model
        .selection()
        .unwrap_or((model.cursor(), model.cursor()));
    offset_utf16(model, start)..offset_utf16(model, end)
}

pub(crate) fn offset_utf16(model: &EditorModel, pos: Pos) -> usize {
    model
        .lines()
        .iter()
        .take(pos.line)
        .map(|line| line.encode_utf16().count() + 1)
        .sum::<usize>()
        + model
            .line(pos.line)
            .unwrap_or("")
            .chars()
            .take(pos.col)
            .map(char::len_utf16)
            .sum::<usize>()
}

pub(crate) fn pos_utf16(model: &EditorModel, offset: usize) -> Pos {
    let source = model.text();
    let byte = byte_at_utf16(&source, offset, false);
    pos_byte(&source, byte)
}

pub(crate) fn pos_byte(source: &str, byte: usize) -> Pos {
    let before = &source[..byte];
    Pos::new(
        before.bytes().filter(|&b| b == b'\n').count(),
        before.rsplit('\n').next().unwrap_or("").chars().count(),
    )
}

pub(crate) fn byte_at_utf16(source: &str, offset: usize, round_up: bool) -> usize {
    let mut utf16 = 0;
    for (byte, ch) in source.char_indices() {
        if utf16 == offset {
            return byte;
        }
        if utf16 + ch.len_utf16() > offset {
            return if round_up { byte + ch.len_utf8() } else { byte };
        }
        utf16 += ch.len_utf16();
    }
    source.len()
}

pub(crate) fn byte_range(source: &str, range: Range<usize>) -> Range<usize> {
    let start = byte_at_utf16(source, range.start, false);
    let end = if range.is_empty() {
        start
    } else {
        byte_at_utf16(source, range.end.max(range.start), true)
    };
    start..end
}

fn select_utf16(model: &mut EditorModel, range: Range<usize>) {
    let start = pos_utf16(model, range.start);
    let end = pos_utf16(model, range.end);
    if model
        .selection()
        .unwrap_or((model.cursor(), model.cursor()))
        != (start, end)
    {
        model.move_to(start.line, start.col, false);
        if start != end {
            model.move_to(end.line, end.col, true);
        }
    }
}

fn replace(model: &mut EditorModel, source: &str, range: Range<usize>, text: &str) {
    let start = pos_byte(source, range.start);
    let end = pos_byte(source, range.end);
    if model
        .selection()
        .unwrap_or((model.cursor(), model.cursor()))
        != (start, end)
    {
        model.move_to(start.line, start.col, false);
        if start != end {
            model.move_to(end.line, end.col, true);
        }
    }
    if text.is_empty() {
        model.delete_selection();
    } else {
        model.insert(text);
    }
}

fn apply_difference(model: &mut EditorModel, target: &str) {
    let source = model.text();
    if source == target {
        return;
    }
    // Composition commits must start a new history step even if the previous
    // edit was a coalescable single character.
    let cursor = model.cursor();
    model.move_to(cursor.line, cursor.col, false);
    let prefix = source
        .chars()
        .zip(target.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let suffix = source[prefix..]
        .chars()
        .rev()
        .zip(target[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    replace(
        model,
        &source,
        prefix..source.len() - suffix,
        &target[prefix..target.len() - suffix],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_preedit_is_one_undo_step_and_preserves_earlier_history() {
        let mut model = EditorModel::new("# ");
        model.move_to(0, 2, false);
        model.insert("A");
        let mut ime = ImeState::default();
        ime.update(&mut model, None, "n", None);
        ime.update(&mut model, None, "nihao", Some(0..5));
        assert_eq!(model.text(), "# Anihao");
        ime.commit(&mut model, None, "你好");
        assert_eq!(model.text(), "# A你好");
        assert!(model.undo());
        assert_eq!(model.text(), "# A");
        assert!(model.undo());
        assert_eq!(model.text(), "# ");
        assert!(model.redo());
        assert!(model.redo());
        assert_eq!(model.text(), "# A你好");
    }

    #[test]
    fn cancelling_composition_does_not_leave_preedit_in_history() {
        let mut model = EditorModel::new("文章");
        model.doc_end(false);
        let mut ime = ImeState::default();
        ime.update(&mut model, None, "pinyin", None);
        ime.commit(&mut model, None, "");
        assert_eq!(model.text(), "文章");
        assert!(!model.can_undo());
        assert_eq!(model.cursor(), Pos::new(0, 2));
    }

    #[test]
    fn cancelling_over_a_selection_restores_original_prose_and_selection() {
        let mut model = EditorModel::new("保留这段文字");
        model.move_to(0, 2, false);
        model.move_to(0, 4, true);
        let original_selection = model.selection();
        let mut ime = ImeState::default();
        ime.update(&mut model, None, "pin", None);
        ime.commit(&mut model, None, "");
        assert_eq!(model.text(), "保留这段文字");
        assert_eq!(model.selection(), original_selection);
        assert!(!model.can_undo());
    }

    #[test]
    fn composition_replaces_cross_line_selection_after_emoji() {
        let mut model = EditorModel::new("🌱开始\n第二段");
        model.move_to(0, 1, false);
        model.move_to(1, 2, true);
        let mut ime = ImeState::default();
        ime.update(&mut model, None, "测试", Some(1..2));
        assert_eq!(selection(&model), 3..4);
        ime.commit(&mut model, None, "新文");
        assert_eq!(model.text(), "🌱新文段");
        assert!(model.undo());
        assert_eq!(model.text(), "🌱开始\n第二段");
    }

    #[test]
    fn explicit_replacement_and_unmark_accept_current_preedit() {
        let mut model = EditorModel::new("a🌱b");
        let mut ime = ImeState::default();
        ime.update(&mut model, Some(1..3), "中", None);
        ime.update(&mut model, Some(1..2), "中文", None);
        ime.unmark(&mut model);
        assert_eq!(model.text(), "a中文b");
        assert_eq!(ime.marked(), None);
        assert!(model.undo());
        assert_eq!(model.text(), "a🌱b");
    }

    #[test]
    fn utf16_ranges_never_split_surrogate_pairs() {
        assert_eq!(byte_range("a🌱b", 2..3), 1..5);
        assert_eq!(byte_range("a🌱b", 2..2), 1..1);
        let model = EditorModel::new("🌱\n中文");
        assert_eq!(pos_utf16(&model, 4), Pos::new(1, 1));
        assert_eq!(offset_utf16(&model, Pos::new(1, 2)), 5);
    }

    #[test]
    fn ordinary_typing_keeps_guises_coalesced_undo() {
        let mut model = EditorModel::new("");
        let mut ime = ImeState::default();
        for text in ["h", "e", "l", "l", "o"] {
            ime.commit(&mut model, None, text);
        }
        assert_eq!(model.text(), "hello");
        assert!(model.undo());
        assert_eq!(model.text(), "");
        assert!(!model.can_undo());
    }
}
