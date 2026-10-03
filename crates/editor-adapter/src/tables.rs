//! Markdown tables with byte ranges into the original editor buffer.
use guise::editor::EditorModel;
use guise::markdown::block::{Block, DocState, classify};
use guise::markdown::layout::{RowPlan, Seg, plan};
use guise::markdown::layout::{byte_for_col, col_for_byte};
use std::ops::Range;

pub enum Navigation {
    NextCell,
    PreviousCell,
    NextRow,
}

fn closing_pipe(source: &str) -> bool {
    let source = source.trim_end();
    source.ends_with('|')
        && source[..source.len() - 1]
            .bytes()
            .rev()
            .take_while(|&byte| byte == b'\\')
            .count()
            % 2
            == 0
}

pub fn escape_pipes(text: &str, prefix: &str) -> String {
    let mut escaped = prefix
        .bytes()
        .rev()
        .take_while(|&byte| byte == b'\\')
        .count()
        % 2
        == 1;
    let mut result = String::new();
    for c in text.chars() {
        if c == '|' && !escaped {
            result.push('\\');
        }
        result.push(c);
        escaped = c == '\\' && !escaped;
    }
    result
}

/// Focus a cell, materializing absent trailing cells before placing the cursor.
pub fn focus_cell(model: &mut EditorModel, line: usize, column: usize) -> bool {
    let source = model.line(line).unwrap_or("").to_owned();
    let ranges = cells(&source);
    let changed = column >= ranges.len();
    if changed {
        let count = column + 1 - ranges.len();
        let suffix = format!(
            "{}{}",
            if closing_pipe(&source) { "" } else { "|" },
            " |".repeat(count)
        );
        model.move_to(line, source.chars().count(), false);
        model.insert(&suffix);
    }
    let source = model.line(line).unwrap_or("");
    let byte = cells(source)
        .get(column)
        .map_or(source.len(), |range| range.start);
    let col = col_for_byte(source, byte);
    model.move_to(line, col, false);
    changed
}

pub fn navigate(model: &mut EditorModel, navigation: Navigation) -> Option<bool> {
    let cursor = model.cursor();
    let (tables, map) = collect(model.lines());
    let table = &tables[map.get(cursor.line).copied().flatten()?];
    if cursor.line == table.start + 1 {
        return None;
    }
    let ranges = &table.rows[cursor.line - table.start];
    let byte = byte_for_col(model.line(cursor.line).unwrap_or(""), cursor.col);
    let column = ranges
        .iter()
        .position(|range| byte <= range.end)
        .unwrap_or(ranges.len().saturating_sub(1));
    let count = table.alignments.len();
    let data_lines = std::iter::once(table.start)
        .chain(table.start + 2..=table.end)
        .collect::<Vec<_>>();
    let row = data_lines.iter().position(|&line| line == cursor.line)?;
    let (target_row, target_column) = match navigation {
        Navigation::PreviousCell if column > 0 => (row, column - 1),
        Navigation::PreviousCell if row > 0 => (row - 1, count - 1),
        Navigation::PreviousCell => return Some(false),
        Navigation::NextCell if column + 1 < count => (row, column + 1),
        Navigation::NextCell => (row + 1, 0),
        Navigation::NextRow => (row + 1, column),
    };
    if let Some(&line) = data_lines.get(target_row) {
        return Some(focus_cell(model, line, target_column));
    }
    // A single insertion is a single undo step and leaves following prose intact.
    let line = table.end;
    let end = model.line(line).unwrap_or("").chars().count();
    model.move_to(line, end, false);
    model.insert(&format!("\n|{}|", vec![" "; count].join("|")));
    focus_cell(model, line + 1, target_column);
    Some(true)
}

pub fn cell_plan(source: &str, reveal: bool) -> RowPlan {
    let mut result = plan(source, &Block::Paragraph, None, reveal);
    if reveal {
        return result;
    }
    let removed = result
        .visible
        .as_bytes()
        .windows(2)
        .enumerate()
        .filter_map(|(i, bytes)| (bytes == b"\\|").then_some(i))
        .collect::<Vec<_>>();
    if removed.is_empty() {
        return result;
    }
    let map = |index: usize| index - removed.iter().filter(|&&byte| byte < index).count();
    let mut segments = Vec::new();
    for segment in &result.segs {
        let mut start = segment.vis.start;
        for &byte in removed.iter().filter(|&&byte| segment.vis.contains(&byte)) {
            if start < byte {
                segments.push(Seg {
                    src: segment.src.start + start - segment.vis.start
                        ..segment.src.start + byte - segment.vis.start,
                    vis: map(start)..map(byte),
                });
            }
            let src = segment.src.start + byte - segment.vis.start;
            segments.push(Seg {
                src: src..src + 1,
                vis: map(byte)..map(byte),
            });
            start = byte + 1;
        }
        if start < segment.vis.end {
            segments.push(Seg {
                src: segment.src.start + start - segment.vis.start..segment.src.end,
                vis: map(start)..map(segment.vis.end),
            });
        } else if segment.vis.is_empty() {
            segments.push(Seg {
                src: segment.src.clone(),
                vis: map(start)..map(start),
            });
        }
    }
    let mut index = 0;
    for run in &mut result.runs {
        let end = index + run.len;
        run.len = map(end) - map(index);
        index = end;
    }
    result.runs.retain(|run| run.len > 0);
    result.visible = result
        .visible
        .char_indices()
        .filter(|(index, _)| !removed.contains(index))
        .map(|(_, c)| c)
        .collect();
    result.segs = segments;
    result
}

/// Clicking immediately before a displayed escaped pipe must insert before
/// its backslash, so editing cannot detach the escape from the literal pipe.
pub fn cell_source_boundary(plan: &RowPlan, source: usize) -> usize {
    if !plan.revealed
        && let Some(segment) = plan.segs.iter().find(|segment| {
            segment.src.end == source
                && segment.src.len() == 1
                && segment.vis.is_empty()
                && plan.visible.as_bytes().get(segment.vis.start) == Some(&b'|')
        })
    {
        segment.src.start
    } else {
        source
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug)]
pub struct Table {
    pub start: usize,
    pub end: usize,
    pub alignments: Vec<Alignment>,
    /// Header and body cells; the delimiter row has no cells.
    pub rows: Vec<Vec<Range<usize>>>,
}

/// Split only unescaped pipes; keep UTF-8 byte offsets, including empty cells.
pub fn cells(line: &str) -> Vec<Range<usize>> {
    let text = line.trim();
    let offset = line.len() - line.trim_start().len();
    let mut separators = Vec::new();
    let mut escaped = false;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'|' && !escaped {
            separators.push(index);
        }
        escaped = byte == b'\\' && !escaped;
    }
    if separators.is_empty() {
        return Vec::new();
    }
    let trailing_pipe = separators.last() == Some(&(text.len() - 1));
    let mut cuts = vec![0];
    cuts.extend(separators.iter().map(|index| index + 1));
    let mut ends = separators;
    ends.push(text.len());
    let mut ranges = cuts
        .into_iter()
        .zip(ends)
        .map(|(start, end)| {
            let cell = &text[start..end];
            let left = cell.len() - cell.trim_start().len();
            let trimmed = cell.trim();
            offset + start + left..offset + start + left + trimmed.len()
        })
        .collect::<Vec<_>>();
    if text.starts_with('|') {
        ranges.remove(0);
    }
    if trailing_pipe {
        ranges.pop();
    }
    ranges
}

fn delimiter(line: &str, count: usize) -> Option<Vec<Alignment>> {
    let ranges = cells(line);
    if ranges.len() != count || count == 0 {
        return None;
    }
    ranges
        .into_iter()
        .map(|range| {
            let marker = &line[range];
            let dashes = marker.trim_start_matches(':').trim_end_matches(':');
            if dashes.is_empty() || !dashes.bytes().all(|byte| byte == b'-') {
                return None;
            }
            Some(if marker.starts_with(':') && marker.ends_with(':') {
                Alignment::Center
            } else if marker.ends_with(':') {
                Alignment::Right
            } else {
                Alignment::Left
            })
        })
        .collect()
}

pub fn collect(lines: &[String]) -> (Vec<Table>, Vec<Option<usize>>) {
    let mut state = DocState::default();
    let eligible = lines
        .iter()
        .map(|line| {
            matches!(
                classify(line, &mut state),
                Block::Table | Block::Paragraph | Block::Rule
            )
        })
        .collect::<Vec<_>>();
    let mut tables = Vec::new();
    let mut membership = vec![None; lines.len()];
    let mut index = 0;
    while index + 1 < lines.len() {
        let header = cells(&lines[index]);
        if eligible[index]
            && eligible[index + 1]
            && let Some(alignments) = delimiter(&lines[index + 1], header.len())
        {
            let start = index;
            let mut rows = vec![header, Vec::new()];
            index += 2;
            while index < lines.len() && eligible[index] {
                let mut row = cells(&lines[index]);
                if row.is_empty() {
                    break;
                }
                row.truncate(alignments.len());
                rows.push(row);
                index += 1;
            }
            membership[start..index].fill(Some(tables.len()));
            tables.push(Table {
                start,
                end: index - 1,
                alignments,
                rows,
            });
        } else {
            index += 1;
        }
    }
    (tables, membership)
}

impl Table {
    /// Short labels stay compact; verbose columns receive more writing space.
    pub fn widths(&self, lines: &[String], available: f32) -> Vec<f32> {
        let weights = (0..self.alignments.len())
            .map(|column| {
                let lengths = self
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(r, row)| row.get(column).map(|range| (r, range)))
                    .map(|(r, range)| {
                        lines[self.start + r][range.clone()]
                            .chars()
                            .map(|c| if c.is_ascii() { 1.0 } else { 2.0 })
                            .sum::<f32>()
                    })
                    .collect::<Vec<_>>();
                let average = lengths.iter().sum::<f32>() / lengths.len().max(1) as f32;
                average.max(4.0).sqrt()
            })
            .collect::<Vec<_>>();
        let total = weights.iter().sum::<f32>();
        let minimum = (available / weights.len() as f32 * 0.65).min(80.0);
        let remaining = (available - minimum * weights.len() as f32).max(0.0);
        weights
            .iter()
            .map(|weight| minimum + remaining * weight / total)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lines(source: &str) -> Vec<String> {
        source.split('\n').map(str::to_owned).collect()
    }

    #[test]
    fn utf8_empty_cells_and_escaped_pipes_keep_source_ranges() {
        let source = " | 中文 | `a\\|b` | | 🌱 | ";
        let ranges = cells(source);
        assert_eq!(
            ranges
                .iter()
                .map(|range| &source[range.clone()])
                .collect::<Vec<_>>(),
            ["中文", "`a\\|b`", "", "🌱"]
        );
        assert_eq!(cells("a | b").len(), 2);
    }

    #[test]
    fn validates_delimiters_alignments_and_excludes_code() {
        let source = lines(
            "```md\n| x | y |\n|---|---|\n```\n\nA | B | C\n:--- | :---: | ---:\n中文 | 数值 | 10\n\n| broken | table |\n|---|\n",
        );
        let (tables, map) = collect(&source);
        assert_eq!(tables.len(), 1);
        assert_eq!(
            tables[0].alignments,
            [Alignment::Left, Alignment::Center, Alignment::Right]
        );
        assert_eq!(tables[0].start, 5);
        assert_eq!(tables[0].end, 7);
        assert_eq!(map[1], None);
        assert_eq!(map[7], Some(0));
    }

    #[test]
    fn unequal_rows_keep_byte_ranges_and_widths_fit() {
        let source = lines(
            "| 名称 | 一列很长的详细说明 |\n|---|---|\n| A |\n| B | 较长内容需要足够的宽度自动换行 | 多余 |\n",
        );
        let (tables, _) = collect(&source);
        assert_eq!(tables[0].rows[2].len(), 1);
        assert_eq!(tables[0].rows[3].len(), 2);
        let widths = tables[0].widths(&source, 700.0);
        assert!((widths.iter().sum::<f32>() - 700.0).abs() < 0.01);
        assert!(widths[1] > widths[0]);
    }

    #[test]
    fn user_document_has_24_tables_with_valid_ranges() {
        let source = include_str!("../../../fixtures/product-definition-v2.md");
        let lines = lines(source);
        let (tables, map) = collect(&lines);
        assert_eq!(tables.len(), 24);
        assert!(
            tables
                .iter()
                .all(|table| (2..=5).contains(&table.alignments.len()))
        );
        for (index, table) in tables.iter().enumerate() {
            assert!((table.widths(&lines, 786.0).iter().sum::<f32>() - 786.0).abs() < 0.1);
            for line in table.start..=table.end {
                assert_eq!(map[line], Some(index));
                for range in &table.rows[line - table.start] {
                    assert!(lines[line].is_char_boundary(range.start));
                    assert!(lines[line].is_char_boundary(range.end));
                }
            }
        }
        assert_eq!(lines.join("\n"), source);
    }

    #[test]
    fn inline_escape_mapping_remains_byte_exact() {
        use guise::markdown::layout::{src_for_vis, vis_for_src};
        let source = "**中文** 与 `a\\|b`";
        let preview = cell_plan(source, false);
        assert_eq!(preview.visible, "中文 与 a|b");
        let pipe = source.find('|').unwrap();
        let visible = vis_for_src(&preview.segs, pipe);
        assert_eq!(&preview.visible[visible..visible + 1], "|");
        assert_eq!(src_for_vis(&preview.segs, visible), pipe);
        assert_eq!(cell_source_boundary(&preview, pipe), pipe - 1);
        assert_eq!(cell_plan(source, true).visible, source);
    }

    #[test]
    fn keyboard_navigation_fills_missing_cells_and_adds_an_undoable_row() {
        let original = "| 名称 | 说明 |\n|---|---|\n| 🌱 |\n\n正文保持不变";
        let mut model = EditorModel::new(original);
        focus_cell(&mut model, 2, 0);
        assert_eq!(navigate(&mut model, Navigation::NextCell), Some(true));
        model.insert("中文");
        assert_eq!(model.line(2), Some("| 🌱 | 中文|"));
        let filled = model.text();
        assert_eq!(navigate(&mut model, Navigation::NextCell), Some(true));
        assert_eq!(model.line(3), Some("| | |"));
        assert_eq!(model.line(5), Some("正文保持不变"));
        model.undo();
        assert_eq!(model.text(), filled);
        assert_eq!(navigate(&mut model, Navigation::PreviousCell), Some(false));
        assert_eq!(model.text(), filled);
    }

    #[test]
    fn missing_cell_without_outer_pipe_and_literal_pipe_input_stay_in_the_grid() {
        let mut model = EditorModel::new("A | B | C\n---|---|---\n中文 | 末尾\\|");
        assert!(focus_cell(&mut model, 2, 2));
        assert_eq!(cells(model.line(2).unwrap()).len(), 3);
        assert_eq!(escape_pipes("a|b", ""), "a\\|b");
        assert_eq!(escape_pipes("a\\|b", ""), "a\\|b");
        assert_eq!(escape_pipes("|", "a\\"), "|");
    }
}
