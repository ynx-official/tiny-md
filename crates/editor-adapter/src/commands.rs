//! Source-preserving commands shared by native menus and shortcuts.
use guise::editor::{EditorModel, Pos};
use guise::markdown::block::{Block, DocState, classify};
use guise::markdown::layout::{byte_for_col, col_for_byte, plan};
use std::ops::Range;

pub fn plain_text(source: &str) -> String {
    let mut state = DocState::default();
    source
        .lines()
        .filter_map(|line| {
            let block = classify(line, &mut state);
            if matches!(block, Block::Fence { .. } | Block::FrontMatter) {
                return None;
            }
            Some(plan(line, &block, None, false).visible)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Copy)]
pub enum BlockStyle {
    Heading(u8),
    Paragraph,
    Quote,
    Bullet,
    Ordered,
    Task,
}

#[derive(Clone, Copy)]
pub enum EditorCommand {
    Wrap(&'static str),
    Link,
    Block(BlockStyle),
    HeadingDelta(i8),
    CodeBlock,
    Table,
    Rule,
    FrontMatter,
    ParagraphAbove,
    ParagraphBelow,
    Indent(bool),
    MoveLine(bool),
    SelectLine,
    DeleteLine,
    DeleteBlock,
    ClearFormat,
    ToggleTask,
    TableEdit(crate::tables::TableCommand),
}

fn selected_lines(model: &EditorModel) -> (usize, usize) {
    model.selection().map_or_else(
        || (model.cursor().line, model.cursor().line),
        |(s, e)| (s.line, e.line - usize::from(e.line > s.line && e.col == 0)),
    )
}

fn replace_lines(model: &mut EditorModel, first: usize, last: usize, text: &str) {
    let end = model.line(last).unwrap_or("").chars().count();
    model.move_to(first, 0, false);
    model.move_to(last, end, true);
    if text.is_empty() {
        model.delete_selection();
    } else {
        model.insert(text);
    }
}

fn content(line: &str) -> &str {
    let block = classify(line, &mut DocState::default());
    let offset = match block {
        Block::Heading { content, .. }
        | Block::Quote { content, .. }
        | Block::Bullet { content, .. }
        | Block::Ordered { content, .. }
        | Block::Task { content, .. } => content,
        _ => 0,
    };
    &line[offset..]
}

pub fn heading_start(model: &EditorModel, line: usize) -> usize {
    match classify(model.line(line).unwrap_or(""), &mut DocState::default()) {
        Block::Heading { content, .. } => content,
        _ => 0,
    }
}

pub fn clamp_heading_cursor(model: &mut EditorModel) {
    if model.selection().is_some() {
        return;
    }
    let cursor = model.cursor();
    let start = heading_start(model, cursor.line);
    if cursor.col < start {
        model.move_to(cursor.line, start, false);
    }
}

pub fn set_block(model: &mut EditorModel, style: BlockStyle) {
    let (first, last) = selected_lines(model);
    let prefix = match style {
        BlockStyle::Heading(level) => format!("{} ", "#".repeat(level.clamp(1, 6) as usize)),
        BlockStyle::Paragraph => String::new(),
        BlockStyle::Quote => "> ".into(),
        BlockStyle::Bullet => "- ".into(),
        BlockStyle::Ordered => "1. ".into(),
        BlockStyle::Task => "- [ ] ".into(),
    };
    let output = model.lines()[first..=last]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let prefix = if matches!(style, BlockStyle::Ordered) {
                format!("{}. ", i + 1)
            } else {
                prefix.clone()
            };
            format!("{prefix}{}", content(line))
        })
        .collect::<Vec<_>>()
        .join("\n");
    replace_lines(model, first, last, &output);
}

pub fn apply(model: &mut EditorModel, command: EditorCommand) {
    let cursor = model.cursor();
    match command {
        EditorCommand::Block(style) => set_block(model, style),
        EditorCommand::HeadingDelta(delta) => {
            let level = match classify(
                model.line(cursor.line).unwrap_or(""),
                &mut DocState::default(),
            ) {
                Block::Heading { level, .. } => level as i8,
                _ => 0,
            };
            let target = if level == 0 {
                1
            } else {
                (level + delta).clamp(0, 6)
            };
            set_block(
                model,
                if target == 0 {
                    BlockStyle::Paragraph
                } else {
                    BlockStyle::Heading(target as u8)
                },
            );
        }
        EditorCommand::CodeBlock => {
            let (first, last) = selected_lines(model);
            let source = model.lines()[first..=last].join("\n");
            let fence = "`".repeat(
                source
                    .split('\n')
                    .map(|line| line.chars().take_while(|&c| c == '`').count())
                    .max()
                    .unwrap_or(0)
                    .max(2)
                    + 1,
            );
            replace_lines(model, first, last, &format!("{fence}\n{source}\n{fence}"));
            model.move_to(first + 1, 0, false);
        }
        EditorCommand::Table | EditorCommand::Rule => {
            let insertion = if matches!(command, EditorCommand::Table) {
                "| 标题 | 标题 |\n| --- | --- |\n|  |  |"
            } else {
                "---"
            };
            let empty = model.line(cursor.line).unwrap_or("").is_empty();
            model.end(false);
            model.insert(&format!(
                "{}{insertion}\n\n",
                if empty { "" } else { "\n\n" }
            ));
            if matches!(command, EditorCommand::Table) {
                model.move_to(cursor.line + if empty { 2 } else { 4 }, 2, false);
            }
        }
        EditorCommand::FrontMatter => {
            model.doc_start(false);
            model.insert("---\ntitle: \n---\n\n");
            model.move_to(1, 7, false);
        }
        EditorCommand::ParagraphAbove => {
            model.move_to(cursor.line, 0, false);
            model.insert("\n");
            model.move_to(cursor.line, 0, false);
        }
        EditorCommand::ParagraphBelow => {
            model.end(false);
            model.insert("\n");
        }
        EditorCommand::Indent(outdent) => {
            let (first, last) = selected_lines(model);
            let output = model.lines()[first..=last]
                .iter()
                .map(|line| {
                    if outdent {
                        let n = line
                            .chars()
                            .take_while(|&c| c == ' ')
                            .count()
                            .min(model.tab_size());
                        line[n..].to_owned()
                    } else {
                        format!("{}{line}", " ".repeat(model.tab_size()))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            replace_lines(model, first, last, &output);
        }
        EditorCommand::MoveLine(down) => {
            let (first, last) = selected_lines(model);
            if (down && last + 1 == model.line_count()) || (!down && first == 0) {
                return;
            }
            let (start, end) = if down {
                (first, last + 1)
            } else {
                (first - 1, last)
            };
            let mut lines = model.lines()[start..=end].to_vec();
            if down {
                lines.rotate_right(1);
            } else {
                lines.rotate_left(1);
            }
            replace_lines(model, start, end, &lines.join("\n"));
            model.move_to(
                if down {
                    cursor.line + 1
                } else {
                    cursor.line - 1
                },
                cursor.col,
                false,
            );
        }
        EditorCommand::SelectLine => model.select_line(),
        EditorCommand::DeleteBlock => {
            let (blocks, map) = crate::code_blocks::collect(model.lines());
            let (tables, table_map) = crate::tables::collect(model.lines());
            let range = if let Some(index) = map[cursor.line] {
                (blocks[index].start, blocks[index].end)
            } else if let Some(index) = table_map[cursor.line] {
                (tables[index].start, tables[index].end)
            } else {
                let current = classify(
                    model.line(cursor.line).unwrap_or(""),
                    &mut DocState::default(),
                );
                let is_prose = |line: &str| {
                    matches!(
                        (&current, classify(line, &mut DocState::default())),
                        (Block::Paragraph, Block::Paragraph)
                            | (Block::Quote { .. }, Block::Quote { .. })
                    )
                };
                let mut first = cursor.line;
                let mut last = cursor.line;
                if is_prose(model.line(cursor.line).unwrap_or("")) {
                    while first > 0 && is_prose(model.line(first - 1).unwrap_or("")) {
                        first -= 1;
                    }
                    while last + 1 < model.line_count()
                        && is_prose(model.line(last + 1).unwrap_or(""))
                    {
                        last += 1;
                    }
                }
                (first, last)
            };
            model.move_to(range.0, 0, false);
            model.move_to(
                range.1,
                model.line(range.1).unwrap_or("").chars().count(),
                true,
            );
            apply(model, EditorCommand::DeleteLine);
        }
        EditorCommand::DeleteLine => {
            let (first, last) = selected_lines(model);
            if last + 1 < model.line_count() {
                model.move_to(first, 0, false);
                model.move_to(last + 1, 0, true);
            } else if first > 0 {
                let end = model.line(first - 1).unwrap_or("").chars().count();
                model.move_to(first - 1, end, false);
                model.doc_end(true);
            } else {
                model.select_all();
            }
            model.delete_selection();
        }
        EditorCommand::ClearFormat => {
            if model.selection().is_none() {
                model.select_word();
            }
            let Some((mut start, mut end)) = model.selection() else {
                return;
            };
            if start.line == end.line {
                let line = model.line(start.line).unwrap_or("");
                let (sb, eb) = (byte_for_col(line, start.col), byte_for_col(line, end.col));
                for marker in ["***", "**", "~~", "==", "*", "_", "`"] {
                    if line[..sb].ends_with(marker) && line[eb..].starts_with(marker) {
                        start.col -= marker.len();
                        end.col += marker.len();
                        break;
                    }
                }
                model.move_to(start.line, start.col, false);
                model.move_to(end.line, end.col, true);
            }
            let source = model.selected_text().unwrap_or_default();
            let output = source
                .split('\n')
                .map(|line| plan(line, &Block::Paragraph, None, false).visible)
                .collect::<Vec<_>>()
                .join("\n");
            if output != source {
                if output.is_empty() {
                    model.delete_selection();
                } else {
                    model.insert(&output);
                }
            }
        }
        _ => {}
    }
}

pub fn matches(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    text.match_indices(query)
        .map(|(start, value)| start..start + value.len())
        .collect()
}

pub fn byte_position(model: &EditorModel, position: Pos) -> usize {
    model
        .lines()
        .iter()
        .take(position.line)
        .map(|line| line.len() + 1)
        .sum::<usize>()
        + byte_for_col(model.line(position.line).unwrap_or(""), position.col)
}

fn position(model: &EditorModel, mut byte: usize) -> Pos {
    for (line, text) in model.lines().iter().enumerate() {
        if byte <= text.len() {
            return Pos::new(line, col_for_byte(text, byte));
        }
        byte -= text.len() + 1;
    }
    Pos::new(
        model.line_count() - 1,
        model.lines().last().map_or(0, |line| line.chars().count()),
    )
}

pub fn select_match(model: &mut EditorModel, range: Range<usize>) {
    let start = position(model, range.start);
    let end = position(model, range.end);
    model.move_to(start.line, start.col, false);
    model.move_to(end.line, end.col, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delete_block_removes_whole_structure_and_undo_restores_source() {
        for (source, line, removed) in [
            ("前文\n> 引用\n> 第二行\n后文", 1, "> 引用\n> 第二行\n"),
            (
                "前文\n\n```rust\n你好 🌱\n```\n\n后文",
                3,
                "```rust\n你好 🌱\n```\n",
            ),
            (
                "前文\n\n| 中文 | 状态 |\n| --- | --- |\n| 🌱 | 完成 |\n\n后文",
                4,
                "| 中文 | 状态 |\n| --- | --- |\n| 🌱 | 完成 |\n",
            ),
            (
                "# 标题\n\n第一行\n第二行 🌱\n\n后文",
                3,
                "第一行\n第二行 🌱\n",
            ),
        ] {
            let mut model = EditorModel::new(source);
            model.move_to(line, 1, false);
            apply(&mut model, EditorCommand::DeleteBlock);
            assert_eq!(model.text(), source.replacen(removed, "", 1));
            assert!(model.undo());
            assert_eq!(model.text(), source);
        }
    }
    #[test]
    fn plain_copy_preserves_unicode_and_code_content() {
        assert_eq!(
            plain_text("# 标题\n\n**中文** 🌱\n\n```rust\nlet value = 1;\n```"),
            "标题\n\n中文 🌱\n\nlet value = 1;"
        );
    }
    #[test]
    fn block_commands_keep_unicode_and_have_one_undo_step() {
        let mut model = EditorModel::new("# 中文 🌱\n正文\n最后");
        model.move_to(0, 0, false);
        model.move_to(1, 2, true);
        apply(&mut model, EditorCommand::Block(BlockStyle::Ordered));
        assert_eq!(model.text(), "1. 中文 🌱\n2. 正文\n最后");
        assert!(model.undo());
        assert_eq!(model.text(), "# 中文 🌱\n正文\n最后");
        model.move_to(0, 0, false);
        model.move_to(1, 2, true);
        apply(&mut model, EditorCommand::MoveLine(true));
        assert_eq!(model.text(), "最后\n# 中文 🌱\n正文");
        assert!(model.undo());
    }
    #[test]
    fn search_maps_multiline_utf8_into_character_positions() {
        let mut model = EditorModel::new("中文 👩‍💻\n中文 🌱");
        let ranges = matches(&model.text(), "中文");
        assert_eq!(ranges.len(), 2);
        select_match(&mut model, ranges[1].clone());
        assert_eq!(model.selected_text().as_deref(), Some("中文"));
        assert_eq!(model.cursor(), Pos::new(1, 2));
        model.insert("替换");
        assert_eq!(model.text(), "中文 👩‍💻\n替换 🌱");
        assert!(model.undo());
        assert_eq!(model.text(), "中文 👩‍💻\n中文 🌱");
    }

    #[test]
    fn hidden_heading_markers_and_clear_format_preserve_surrounding_text() {
        let mut model = EditorModel::new("### 中文标题\n前文 **重点** 后文");
        clamp_heading_cursor(&mut model);
        assert_eq!(model.cursor(), Pos::new(0, 4));
        model.insert("新");
        assert_eq!(model.line(0), Some("### 新中文标题"));
        model.move_to(1, 5, false);
        model.move_to(1, 7, true);
        apply(&mut model, EditorCommand::ClearFormat);
        assert_eq!(model.line(1), Some("前文 重点 后文"));
        model.undo();
        assert_eq!(model.line(1), Some("前文 **重点** 后文"));
    }
}
