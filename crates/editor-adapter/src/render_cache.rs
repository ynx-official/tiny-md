//! Content caches survive scrolling, caret changes and presentation switches.
use crate::{
    code_blocks::{self, CodeBlock},
    tables::{self, Table},
};
use guise::editor::{Highlighter, Language, LineState, TokenKind};
use guise::markdown::block::{Block, DocState, classify};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    rc::Rc,
};

pub(crate) struct ParsedDocument {
    pub lines: Vec<String>,
    pub blocks: Vec<Block>,
    pub languages: Vec<Option<String>>,
    pub tables: Vec<Table>,
    pub table_membership: Vec<Option<usize>>,
    pub code: Vec<CodeBlock>,
    pub code_membership: Vec<Option<usize>>,
    pub code_signatures: Vec<u64>,
    pub source_tokens: Vec<Vec<(Range<usize>, TokenKind)>>,
    pub source_signatures: Vec<u64>,
}

impl ParsedDocument {
    fn parse(lines: &[String]) -> Self {
        let mut state = DocState::default();
        let mut languages = Vec::with_capacity(lines.len());
        let blocks = lines
            .iter()
            .map(|line| {
                languages.push(state.fence_lang().map(str::to_owned));
                classify(line, &mut state)
            })
            .collect();
        let (tables, table_membership) = tables::collect(lines);
        let (code, code_membership) = code_blocks::collect(lines);
        let code_signatures = code
            .iter()
            .map(|block| fingerprint(&(block.language.as_str(), block.source.as_str())))
            .collect();
        let mut state = LineState::default();
        let source_tokens: Vec<_> = lines
            .iter()
            .map(|line| Language::Markdown.line(line, &mut state))
            .collect();
        let source_signatures = source_tokens.iter().map(fingerprint).collect();
        Self {
            lines: lines.to_vec(),
            blocks,
            languages,
            tables,
            table_membership,
            code,
            code_membership,
            code_signatures,
            source_tokens,
            source_signatures,
        }
    }
}

#[derive(Default)]
pub(crate) struct DocumentCache {
    parsed: Option<Rc<ParsedDocument>>,
}

impl DocumentCache {
    pub fn current(&self) -> Option<Rc<ParsedDocument>> {
        self.parsed.clone()
    }
    pub fn update(&mut self, lines: &[String]) -> (Rc<ParsedDocument>, bool) {
        if let Some(parsed) = self.parsed.as_ref().filter(|parsed| parsed.lines == lines) {
            return (parsed.clone(), false);
        }
        let parsed = Rc::new(ParsedDocument::parse(lines));
        self.parsed = Some(parsed.clone());
        (parsed, true)
    }
}

pub(crate) struct RowSlots<K, T> {
    entries: Vec<Option<(K, Rc<T>)>>,
}

impl<K, T> Default for RowSlots<K, T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<K: PartialEq, T> RowSlots<K, T> {
    pub fn rebase(&mut self, old: &[String], new: &[String]) {
        if self.entries.len() != old.len() {
            self.entries = (0..new.len()).map(|_| None).collect();
            return;
        }
        let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        self.entries.splice(
            prefix..old.len() - suffix,
            (prefix..new.len() - suffix).map(|_| None),
        );
    }
    pub fn clear(&mut self) {
        for slot in &mut self.entries {
            *slot = None;
        }
    }
    pub fn get(&self, index: usize, key: &K) -> Option<Rc<T>> {
        self.entries
            .get(index)?
            .as_ref()
            .filter(|(cached, _)| cached == key)
            .map(|(_, row)| row.clone())
    }
    pub fn insert(&mut self, index: usize, key: K, value: T) -> Rc<T> {
        let value = Rc::new(value);
        self.entries[index] = Some((key, value.clone()));
        value
    }
}

#[derive(Debug)]
pub(crate) struct VisibleRows {
    pub range: Range<usize>,
    pub before: f32,
    pub after: f32,
}

pub(crate) fn visible_rows(
    count: usize,
    height: impl Fn(usize) -> f32,
    top: f32,
    viewport: f32,
    margin: f32,
) -> VisibleRows {
    let lower = (top - margin).max(0.0);
    let upper = top.max(0.0) + viewport.max(1.0) + margin;
    let mut y = 0.0;
    let mut start = count;
    let mut end = count;
    let mut before = 0.0;
    let mut visible_end = 0.0;
    for i in 0..count {
        let h = height(i);
        if h > 0.0 && y + h > lower && y < upper {
            if start == count {
                start = i;
                before = y;
            }
            end = i + 1;
            visible_end = y + h;
        }
        y += h;
    }
    if start == count {
        before = y;
        visible_end = y;
    }
    VisibleRows {
        range: start..end,
        before,
        after: (y - visible_end).max(0.0),
    }
}

pub(crate) fn fingerprint(value: &impl Hash) -> u64 {
    let mut hash = DefaultHasher::new();
    value.hash(&mut hash);
    hash.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_owned).collect()
    }

    #[test]
    fn unchanged_content_reuses_structure_and_mode_switches_do_not_discard_blocks() {
        let text = lines(
            "# 标题\n\n| a | b |\n| --- | --- |\n| 中文 | 🌱 |\n\n```mermaid\nflowchart LR\nA --> B\n```",
        );
        let mut cache = DocumentCache::default();
        let (first, changed) = cache.update(&text);
        assert!(changed);
        assert_eq!(first.tables.len(), 1);
        assert_eq!(first.code.len(), 1);
        let (second, changed) = cache.update(&text);
        assert!(!changed);
        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn editing_a_fence_updates_classification_of_unchanged_following_lines() {
        let mut cache = DocumentCache::default();
        let (old, _) = cache.update(&lines("paragraph\nsecond\n```"));
        let (new, changed) = cache.update(&lines("```rust\nsecond\n```"));
        assert!(changed);
        assert_eq!(old.blocks[1], Block::Paragraph);
        assert_eq!(new.blocks[1], Block::CodeLine);
        assert_eq!(new.languages[1].as_deref(), Some("rust"));
        assert_ne!(new.source_signatures[1], old.source_signatures[1]);
    }

    #[test]
    fn cached_rows_are_reused_and_width_or_syntax_changes_require_new_layout() {
        let mut rows = RowSlots::default();
        rows.rebase(&[], &lines("中文"));
        let key = ("中文", 800, false);
        let first = rows.insert(0, key, "measured glyphs");
        assert!(Rc::ptr_eq(&first, &rows.get(0, &key).unwrap()));
        assert!(rows.get(0, &("中文", 600, false)).is_none());
        assert!(rows.get(0, &("中文", 800, true)).is_none());
    }

    #[test]
    fn insertion_reuses_unchanged_suffix_layout_and_deletion_restores_its_index() {
        let old = lines("first\n中文\nlast");
        let new = lines("first\ninserted\n中文\nlast");
        let mut rows = RowSlots::default();
        rows.rebase(&[], &old);
        let glyphs = rows.insert(1, "中文", 42);
        rows.rebase(&old, &new);
        assert!(Rc::ptr_eq(&glyphs, &rows.get(2, &"中文").unwrap()));
        rows.rebase(&new, &old);
        assert!(Rc::ptr_eq(&glyphs, &rows.get(1, &"中文").unwrap()));
    }

    #[test]
    fn viewport_limits_nodes_but_retains_full_scroll_height_and_large_diagrams() {
        let heights = [100.0, 0.0, 800.0, 20.0, 20.0];
        let visible = visible_rows(heights.len(), |i| heights[i], 400.0, 300.0, 100.0);
        assert_eq!(visible.range, 2..3);
        assert_eq!(visible.before, 100.0);
        assert_eq!(visible.after, 40.0);
        let all = visible_rows(heights.len(), |i| heights[i], 0.0, 2000.0, 0.0);
        assert_eq!(all.range, 0..5);
        assert_eq!(all.before + all.after, 0.0);
    }
}
