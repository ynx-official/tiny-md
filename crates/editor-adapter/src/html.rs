//! A deliberately bounded HTML presentation layer. Source bytes remain authoritative.
//! Markdown identifies protected regions; the HTML tokenizer supplies decoded attributes
//! and byte spans. We never serialize a DOM back into the editor's text model.
use guise::markdown::{
    block::Block,
    inline::InlineStyle,
    layout::{self, RowPlan, RunAttrs, Seg},
};
use html5gum::{Token, Tokenizer, emitters::default::DefaultEmitter};
use markdown::mdast::Node;
use std::{collections::HashMap, ops::Range};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub mark: bool,
    pub underline: bool,
    pub script: i8,
    pub color: Option<u32>,
    pub background: Option<u32>,
    pub align: Option<u8>,
    pub link: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Tag {
    pub name: String,
    pub range: Range<usize>,
    pub attrs: HashMap<String, String>,
    pub close: bool,
    pub mate: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Replacement {
    range: Range<usize>,
    text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Details {
    pub range: Range<usize>,
    pub body: Range<usize>,
    pub open: bool,
    pub key: u64,
    pub label: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct TableCell {
    pub content: Range<usize>,
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
    pub header: bool,
    pub align: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub range: Range<usize>,
    pub cells: Vec<TableCell>,
    pub rows: usize,
    pub cols: usize,
    pub signature: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ImageGroup {
    pub range: Range<usize>,
    pub breaks: Vec<usize>,
    pub align: u8,
    pub signature: u64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Document {
    replacements: Vec<Replacement>,
    styles: Vec<(Range<usize>, Style)>,
    literal: Vec<Range<usize>>,
    pub tags: Vec<Tag>,
    pub details: Vec<Details>,
    pub tables: Vec<Table>,
    pub image_groups: Vec<ImageGroup>,
    pub comments: Vec<Range<usize>>,
    pub offsets: Vec<usize>,
    pub signatures: Vec<u64>,
}

pub(crate) fn parse_root(source: &str) -> Option<Node> {
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.frontmatter = true;
    markdown::to_mdast(source, &options).ok()
}

fn supported(name: &str) -> bool {
    matches!(
        name,
        "br" | "img"
            | "b"
            | "strong"
            | "i"
            | "em"
            | "del"
            | "s"
            | "code"
            | "u"
            | "mark"
            | "sub"
            | "sup"
            | "kbd"
            | "a"
            | "p"
            | "div"
            | "span"
            | "details"
            | "summary"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
            | "caption"
    )
}

pub(crate) fn safe_link(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    match reqwest::Url::parse(value.trim()) {
        Ok(url) => matches!(url.scheme(), "http" | "https" | "mailto"),
        Err(_) => !value.trim().starts_with("//") && !value.contains(':'),
    }
}

fn color(value: &str) -> Option<u32> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 6 {
            return u32::from_str_radix(hex, 16).ok();
        }
        if hex.len() == 3 {
            let n = u32::from_str_radix(hex, 16).ok()?;
            return Some(((n & 0xf00) << 8 | (n & 0xf0) << 4 | (n & 0xf)) * 17);
        }
    }
    Some(match value.to_ascii_lowercase().as_str() {
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "black" => 0,
        "white" => 0xffffff,
        "gray" | "grey" => 0x808080,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "yellow" => 0xffff00,
        "teal" => 0x008080,
        "navy" => 0x000080,
        _ => return None,
    })
}

fn alignment(value: &str) -> Option<u8> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" => Some(0),
        "center" => Some(1),
        "right" => Some(2),
        _ => None,
    }
}

fn tag_style(tag: &Tag) -> Style {
    let mut s = Style {
        bold: matches!(tag.name.as_str(), "b" | "strong" | "th" | "summary"),
        italic: matches!(tag.name.as_str(), "i" | "em"),
        strike: matches!(tag.name.as_str(), "del" | "s"),
        code: matches!(tag.name.as_str(), "code" | "kbd"),
        underline: tag.name == "u",
        mark: tag.name == "mark",
        script: match tag.name.as_str() {
            "sub" => 1,
            "sup" => -1,
            _ => 0,
        },
        align: tag.attrs.get("align").and_then(|v| alignment(v)),
        link: tag
            .attrs
            .get("href")
            .filter(|v| tag.name == "a" && safe_link(v))
            .cloned(),
        ..Default::default()
    };
    if let Some(css) = tag.attrs.get("style") {
        for declaration in css.split(';') {
            let Some((key, value)) = declaration.split_once(':') else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "color" => s.color = color(value),
                "background-color" => s.background = color(value),
                "text-align" => s.align = alignment(value),
                "font-weight" => {
                    s.bold |= matches!(value.trim(), "bold" | "600" | "700" | "800" | "900")
                }
                "font-style" => s.italic |= value.trim() == "italic",
                "text-decoration" => {
                    s.underline |= value.split_whitespace().any(|v| v == "underline");
                    s.strike |= value.split_whitespace().any(|v| v == "line-through");
                }
                _ => {}
            }
        }
    }
    s
}

// Decode complete references separately so every transformed character retains an
// exact source span, including multi-codepoint entities and references at EOF.
fn collect_entities(
    fragment: &str,
    offset: usize,
    markdown_escapes: bool,
    out: &mut Vec<Replacement>,
) {
    for (start, _) in fragment.match_indices('&') {
        if markdown_escapes
            && fragment[..start]
                .bytes()
                .rev()
                .take_while(|&b| b == b'\\')
                .count()
                % 2
                == 1
        {
            continue;
        }
        let rest = &fragment[start + 1..];
        let length = rest
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'#')
            .count();
        if length == 0 || rest.as_bytes().get(length) != Some(&b';') {
            continue;
        }
        let name = &rest[..length];
        let numeric = name
            .strip_prefix("#x")
            .or_else(|| name.strip_prefix("#X"))
            .map(|s| (s, 16))
            .or_else(|| name.strip_prefix('#').map(|s| (s, 10)));
        let decoded = if let Some((digits, radix)) = numeric {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                None
            } else if u32::from_str_radix(digits, radix).is_ok() {
                Some(markdown::decode_numeric(digits, radix))
            } else {
                Some(char::REPLACEMENT_CHARACTER.into())
            }
        } else {
            markdown::decode_named(name, true)
        };
        if let Some(text) = decoded {
            out.push(Replacement {
                range: offset + start..offset + start + length + 2,
                text,
            });
        }
    }
}

impl Document {
    pub fn parse(source: &str, root: Option<&Node>) -> Self {
        let mut result = Self {
            offsets: std::iter::once(0)
                .chain(source.match_indices('\n').map(|(i, _)| i + 1))
                .collect(),
            ..Default::default()
        };
        fn visit(node: &Node, source: &str, result: &mut Document, inline: bool) {
            if let Some(p) = node.position() {
                let range = p.start.offset..p.end.offset;
                if matches!(node, Node::Html(_) | Node::Text(_)) {
                    let is_html = matches!(node, Node::Html(_));
                    if let Some(fragment) = source.get(range.clone()) {
                        if !is_html {
                            collect_entities(fragment, range.start, true, &mut result.replacements);
                            return;
                        }
                        let mut emitter = DefaultEmitter::<usize>::new_with_span();
                        emitter.naively_switch_states(true);
                        for token in Tokenizer::new_with_emitter(fragment, emitter).flatten() {
                            match token {
                                Token::StartTag(tag) if is_html => {
                                    result.tags.push(Tag {
                                        name: String::from_utf8_lossy(&tag.name).into_owned(),
                                        range: range.start + tag.span.start
                                            ..range.start + tag.span.end,
                                        attrs: tag
                                            .attributes
                                            .into_iter()
                                            .map(|(key, value)| {
                                                (
                                                    String::from_utf8_lossy(&key).into_owned(),
                                                    String::from_utf8_lossy(&value.value)
                                                        .into_owned(),
                                                )
                                            })
                                            .collect(),
                                        close: false,
                                        mate: None,
                                    });
                                }
                                Token::EndTag(tag) if is_html => {
                                    result.tags.push(Tag {
                                        name: String::from_utf8_lossy(&tag.name).into_owned(),
                                        range: range.start + tag.span.start
                                            ..range.start + tag.span.end,
                                        attrs: HashMap::new(),
                                        close: true,
                                        mate: None,
                                    });
                                }
                                Token::String(text) => {
                                    if let Some(raw) = fragment.get(text.span.start..text.span.end)
                                    {
                                        collect_entities(
                                            raw,
                                            range.start + text.span.start,
                                            !is_html,
                                            &mut result.replacements,
                                        );
                                    }
                                }
                                Token::Comment(comment) => {
                                    let span = comment.span.start..comment.span.end;
                                    if fragment.get(span.clone()).is_some_and(|s| {
                                        s.starts_with("<!--") && s.ends_with("-->")
                                    }) {
                                        let range =
                                            range.start + span.start..range.start + span.end;
                                        result.comments.push(range.clone());
                                        result.replacements.push(Replacement {
                                            range,
                                            text: String::new(),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // Block HTML keeps Markdown punctuation literal, as specified by CommonMark.
                    if is_html && !inline {
                        result.literal.push(range);
                    }
                }
            }
            if let Some(children) = node.children() {
                for child in children {
                    visit(
                        child,
                        source,
                        result,
                        matches!(
                            node,
                            Node::Paragraph(_)
                                | Node::Heading(_)
                                | Node::Emphasis(_)
                                | Node::Strong(_)
                                | Node::Delete(_)
                                | Node::Link(_)
                                | Node::TableCell(_)
                        ),
                    );
                }
            }
        }
        if let Some(root) = root {
            visit(root, source, &mut result, false);
        }
        result.tags.sort_by_key(|tag| tag.range.start);
        let mut stack: Vec<usize> = Vec::new();
        for index in 0..result.tags.len() {
            let tag = &result.tags[index];
            if matches!(
                tag.name.as_str(),
                "br" | "img" | "hr" | "input" | "meta" | "link"
            ) {
                continue;
            }
            if tag.close {
                if let Some(open) = stack.pop() {
                    if result.tags[open].name == tag.name {
                        result.tags[open].mate = Some(index);
                        result.tags[index].mate = Some(open);
                    } else {
                        stack.clear();
                    }
                }
            } else if stack.len() < 128 {
                stack.push(index);
            }
        }
        for tag in &result.tags {
            if !supported(&tag.name)
                || (tag.mate.is_none() && !matches!(tag.name.as_str(), "br" | "img"))
            {
                continue;
            }
            let text = if tag.name == "br" { "\n" } else { "" };
            result.replacements.push(Replacement {
                range: tag.range.clone(),
                text: text.into(),
            });
            if !tag.close
                && let Some(mate) = tag.mate
            {
                let close = &result.tags[mate];
                result
                    .styles
                    .push((tag.range.end..close.range.start, tag_style(tag)));
            }
        }
        // Inline p/div boundaries produce real line breaks without modifying the source line.
        for tag in &result.tags {
            if tag.close && matches!(tag.name.as_str(), "p" | "div") && tag.mate.is_some() {
                let rest = source[tag.range.end..].split('\n').next().unwrap_or("");
                if !rest.trim().is_empty()
                    && !rest.trim_start().starts_with("</")
                    && let Some(r) = result
                        .replacements
                        .iter_mut()
                        .find(|r| r.range == tag.range)
                {
                    r.text = "\n".into();
                }
            }
        }
        result.collect_containers(source);
        // A source row owns one native grid. Ambiguous sibling grids keep their
        // complete source instead of hiding one behind the other's layout.
        let mut table_lines = HashMap::new();
        for table in &result.tables {
            *table_lines
                .entry(result.line(table.range.start))
                .or_insert(0) += 1;
        }
        result.tables.retain(|table| {
            table_lines[&(result.offsets.partition_point(|&p| p <= table.range.start) - 1)] == 1
        });
        // Invalid or nested grids remain editable source instead of concatenated cells.
        for tag in &result.tags {
            if tag.name == "table"
                && !tag.close
                && let Some(mate) = tag.mate
            {
                let range = tag.range.start..result.tags[mate].range.end;
                if !result.tables.iter().any(|t| t.range == range) {
                    result
                        .replacements
                        .retain(|r| r.range.end <= range.start || r.range.start >= range.end);
                }
            }
        }
        result.replacements.sort_by_key(|r| r.range.start);
        result.styles.sort_by_key(|(range, _)| range.start);
        result.collect_image_groups(source);
        result.signatures = result
            .offsets
            .iter()
            .enumerate()
            .map(|(line, &start)| {
                let end = result
                    .offsets
                    .get(line + 1)
                    .copied()
                    .unwrap_or(source.len() + 1)
                    - 1;
                crate::render_cache::fingerprint(&(
                    result
                        .replacements
                        .iter()
                        .filter(|r| r.range.start <= end && r.range.end > start)
                        .collect::<Vec<_>>(),
                    result
                        .styles
                        .iter()
                        .filter(|(r, _)| r.start <= end && r.end > start)
                        .collect::<Vec<_>>(),
                ))
            })
            .collect();
        result
    }

    fn collect_containers(&mut self, source: &str) {
        for (index, tag) in self.tags.iter().enumerate() {
            if tag.close {
                continue;
            }
            let Some(mate) = tag.mate else {
                continue;
            };
            let end = &self.tags[mate];
            if tag.name == "details" {
                let summary = self.tags[index + 1..mate].iter().find(|t| {
                    t.name == "summary"
                        && !t.close
                        && t.mate.is_some()
                        && !self.tags[index + 1..mate].iter().any(|parent| {
                            parent.name == "details"
                                && !parent.close
                                && parent.range.start < t.range.start
                                && parent
                                    .mate
                                    .is_some_and(|end| self.tags[end].range.start > t.range.start)
                        })
                });
                let (label, body_start) = if let Some(summary) = summary {
                    let summary_end = &self.tags[summary.mate.unwrap()];
                    (
                        summary.range.end
                            + source[summary.range.end..summary_end.range.start]
                                .find(|c: char| !c.is_whitespace())
                                .unwrap_or(0),
                        summary_end.range.end,
                    )
                } else {
                    // A generated label is presentation only, mapped to the opening tag.
                    if let Some(r) = self.replacements.iter_mut().find(|r| r.range == tag.range) {
                        r.text = "详细信息".into();
                    }
                    (tag.range.start, tag.range.end)
                };
                self.details.push(Details {
                    label,
                    range: tag.range.start..end.range.end,
                    body: body_start..end.range.start,
                    open: tag.attrs.contains_key("open"),
                    key: crate::render_cache::fingerprint(&(
                        tag.range.start,
                        &source[tag.range.clone()],
                    )),
                });
            }
            if tag.name != "table" {
                continue;
            }
            // Nested tables are kept as source; a rectangular grid must never lose cells.
            if self.tags[index + 1..mate].iter().any(|t| t.name == "table")
                || self.tags[..index].iter().any(|parent| {
                    parent.name == "table"
                        && !parent.close
                        && parent.mate.is_some_and(|end| end > mate)
                })
            {
                continue;
            }
            let mut cells = Vec::new();
            let mut occupied: Vec<Vec<bool>> = Vec::new();
            let mut rows = 0;
            let mut cols = 0;
            'rows: for tr in self.tags[index + 1..mate]
                .iter()
                .filter(|t| t.name == "tr" && !t.close && t.mate.is_some())
            {
                let tr_end = &self.tags[tr.mate.unwrap()];
                let row = rows;
                rows += 1;
                if rows > 2048 {
                    cells.clear();
                    break;
                }
                occupied.resize_with(occupied.len().max(rows), || vec![false; 128]);
                let mut col = 0;
                for cell in self.tags.iter().filter(|t| {
                    matches!(t.name.as_str(), "td" | "th")
                        && !t.close
                        && t.mate.is_some()
                        && t.range.start >= tr.range.end
                        && t.range.end <= tr_end.range.start
                }) {
                    let span = |name: &str| {
                        cell.attrs
                            .get(name)
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1)
                            .clamp(1, 128)
                    };
                    let colspan = span("colspan");
                    let rowspan = span("rowspan");
                    while col + colspan <= 128
                        && occupied[row][col..col + colspan].iter().any(|&v| v)
                    {
                        col += 1;
                    }
                    if col + colspan > 128 {
                        cells.clear();
                        break 'rows;
                    }
                    occupied.resize_with(occupied.len().max(row + rowspan), || vec![false; 128]);
                    for occupied_row in &mut occupied[row..row + rowspan] {
                        occupied_row[col..col + colspan].fill(true);
                    }
                    cells.push(TableCell {
                        content: cell.range.end..self.tags[cell.mate.unwrap()].range.start,
                        row,
                        col,
                        rowspan,
                        colspan,
                        header: cell.name == "th",
                        align: tag_style(cell)
                            .align
                            .or_else(|| tag_style(tr).align)
                            .unwrap_or(0),
                    });
                    col += colspan;
                    cols = cols.max(col);
                }
            }
            if !cells.is_empty() {
                if let Some(caption) = self.tags[index + 1..mate]
                    .iter()
                    .find(|t| t.name == "caption" && !t.close && t.mate.is_some())
                {
                    for cell in &mut cells {
                        cell.row += 1;
                    }
                    cells.insert(
                        0,
                        TableCell {
                            content: caption.range.end
                                ..self.tags[caption.mate.unwrap()].range.start,
                            row: 0,
                            col: 0,
                            rowspan: 1,
                            colspan: cols,
                            header: false,
                            align: 1,
                        },
                    );
                    rows = rows.max(occupied.len()) + 1;
                }
                self.tables.push(Table {
                    range: tag.range.start..end.range.end,
                    cells,
                    rows: rows.max(occupied.len()),
                    cols,
                    signature: crate::render_cache::fingerprint(
                        &&source[tag.range.start..end.range.end],
                    ),
                });
            }
        }
    }

    pub fn line(&self, offset: usize) -> usize {
        self.offsets
            .partition_point(|&p| p <= offset)
            .saturating_sub(1)
    }

    pub fn image_tags(&self) -> impl Iterator<Item = &Tag> {
        self.tags.iter().filter(|t| {
            t.name == "img"
                && !t.close
                && !self.tags.iter().any(|parent| {
                    matches!(
                        parent.name.as_str(),
                        "script" | "style" | "iframe" | "object"
                    ) && !parent.close
                        && parent.range.start < t.range.start
                        && parent
                            .mate
                            .is_some_and(|end| self.tags[end].range.end > t.range.end)
                })
        })
    }

    pub fn alignment_at(&self, offset: usize) -> u8 {
        self.styles
            .iter()
            .filter(|(r, _)| r.contains(&offset))
            .filter_map(|(_, s)| s.align)
            .next_back()
            .unwrap_or(0)
    }

    pub fn link_at(&self, offset: usize) -> Option<String> {
        self.styles
            .iter()
            .filter(|(range, _)| range.contains(&offset))
            .filter_map(|(_, style)| style.link.clone())
            .next_back()
    }

    fn collect_image_groups(&mut self, source: &str) {
        for (index, tag) in self.tags.iter().enumerate() {
            if tag.close || !matches!(tag.name.as_str(), "p" | "div") {
                continue;
            }
            let Some(mate) = tag.mate else {
                continue;
            };
            let end = &self.tags[mate];
            // Table cells have their own layout. Only image-only containers can
            // be collapsed into a flow; mixed text and unsupported content stay visible.
            if self.tags[..index]
                .iter()
                .any(|t| t.name == "table" && !t.close && t.mate.is_some_and(|m| m > mate))
            {
                continue;
            }
            let children = &self.tags[index + 1..mate];
            if !children
                .iter()
                .any(|t| t.name == "img" && !t.close && t.attrs.contains_key("src"))
                || children.iter().any(|t| match t.name.as_str() {
                    "a" | "span" => t.mate.is_none(),
                    "img" => t.close || !t.attrs.contains_key("src"),
                    "br" => t.close,
                    _ => true,
                })
            {
                continue;
            }
            let mut spans: Vec<_> = children
                .iter()
                .map(|t| t.range.clone())
                .chain(
                    self.comments
                        .iter()
                        .filter(|r| r.start >= tag.range.end && r.end <= end.range.start)
                        .cloned(),
                )
                .collect();
            spans.sort_by_key(|r| r.start);
            let mut at = tag.range.end;
            let mut only_images = true;
            for span in spans {
                if !source[at..span.start].trim().is_empty() {
                    only_images = false;
                    break;
                }
                at = span.end;
            }
            if !only_images || !source[at..end.range.start].trim().is_empty() {
                continue;
            }
            let range = tag.range.start..end.range.end;
            self.image_groups.push(ImageGroup {
                signature: crate::render_cache::fingerprint(&(
                    range.clone(),
                    &source[range.clone()],
                )),
                range,
                breaks: children
                    .iter()
                    .filter(|t| t.name == "br")
                    .map(|t| t.range.start)
                    .collect(),
                align: self.alignment_at(tag.range.end),
            });
        }
        let mut starts = HashMap::new();
        for group in &self.image_groups {
            *starts.entry(self.line(group.range.start)).or_insert(0) += 1;
        }
        self.image_groups
            .retain(|g| starts[&(self.offsets.partition_point(|&p| p <= g.range.start) - 1)] == 1);
    }

    pub fn plan(
        &self,
        source: &str,
        offset: usize,
        block: &Block,
        lang: Option<&str>,
        reveal: bool,
        hidden: &[Range<usize>],
    ) -> (RowPlan, Vec<Style>) {
        if reveal
            || matches!(
                block,
                Block::CodeLine | Block::Fence { .. } | Block::FrontMatter
            )
        {
            let p = layout::plan(source, block, lang, reveal);
            let extras = vec![Style::default(); p.runs.len()];
            return (p, extras);
        }
        let end = offset + source.len();
        let replacements: Vec<_> = self
            .replacements
            .iter()
            .filter(|r| r.range.start < end && r.range.end > offset)
            .collect();
        let styles: Vec<_> = self
            .styles
            .iter()
            .filter(|(r, _)| r.start < end && r.end > offset)
            .collect();
        if replacements.is_empty() && styles.is_empty() && hidden.is_empty() {
            let p = layout::plan(source, block, lang, false);
            let extras = vec![Style::default(); p.runs.len()];
            return (p, extras);
        }
        let mut masked = source.as_bytes().to_vec();
        for r in &replacements {
            masked[r.range.start.saturating_sub(offset)..(r.range.end - offset).min(source.len())]
                .fill(b' ');
        }
        // Protect only HTML code contents; surrounding Markdown still uses its own grammar.
        for (range, style) in &styles {
            if style.code {
                masked[range.start.saturating_sub(offset)..(range.end - offset).min(source.len())]
                    .fill(b' ');
            }
        }
        let masked = String::from_utf8(masked).expect("only whole source spans are masked");
        let literal = self
            .literal
            .iter()
            .any(|r| r.start <= offset && r.end >= end);
        let base = layout::plan(
            &masked,
            if literal { &Block::CodeLine } else { block },
            lang,
            false,
        );
        let mut attrs = vec![(InlineStyle::default(), false, false); base.visible.len()];
        let mut at = 0;
        for run in &base.runs {
            attrs[at..at + run.len].fill((
                if literal {
                    InlineStyle::default()
                } else {
                    run.style
                },
                run.marker,
                run.dim,
            ));
            at += run.len;
        }
        let mut out = RowPlan {
            kind: if literal {
                layout::RowKind::Paragraph
            } else {
                base.kind.clone()
            },
            visible: String::new(),
            runs: Vec::new(),
            segs: Vec::new(),
            links: base.links.clone(),
            revealed: false,
        };
        let mut extras = Vec::new();
        let mut cuts = vec![0, source.len()];
        for seg in &base.segs {
            cuts.extend([seg.src.start, seg.src.end]);
        }
        for range in replacements
            .iter()
            .map(|r| &r.range)
            .chain(styles.iter().map(|(r, _)| r))
            .chain(hidden.iter())
        {
            cuts.extend([
                range.start.saturating_sub(offset).min(source.len()),
                range.end.saturating_sub(offset).min(source.len()),
            ]);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut style_index = 0;
        let mut active: Vec<&(Range<usize>, Style)> = Vec::new();
        for cut in cuts.windows(2) {
            let (byte, next) = (cut[0], cut[1]);
            let absolute = offset + byte;
            let replacement = replacements
                .get(replacements.partition_point(|r| r.range.end <= absolute))
                .filter(|r| r.range.contains(&absolute));
            let base_seg = base
                .segs
                .get(base.segs.partition_point(|s| s.src.end <= byte));
            let base_vis = base_seg.map_or(base.visible.len(), |s| {
                (s.vis.start + byte.saturating_sub(s.src.start)).min(s.vis.end)
            });
            let (mut inline, marker, dim) = attrs.get(base_vis).copied().unwrap_or_default();
            let mut extra = Style::default();
            while style_index < styles.len() && styles[style_index].0.start <= absolute {
                active.push(styles[style_index]);
                style_index += 1;
            }
            active.retain(|(range, _)| range.end > absolute);
            for (_, s) in &active {
                inline.bold |= s.bold;
                inline.italic |= s.italic;
                inline.strike |= s.strike;
                inline.code |= s.code;
                inline.highlight |= s.mark;
                inline.link |= s.link.is_some();
                extra.underline |= s.underline;
                if s.script != 0 {
                    extra.script = s.script;
                }
                extra.color = s.color.or(extra.color);
                extra.background = s.background.or(extra.background);
                extra.align = s.align.or(extra.align);
                extra.link = s.link.clone().or(extra.link);
            }
            let hidden_source = hidden.iter().any(|r| r.contains(&absolute));
            let base_hidden = base_seg.is_some_and(|seg| seg.vis.is_empty());
            let text = if hidden_source {
                ""
            } else if let Some(r) = replacement {
                if r.range.start == absolute {
                    &r.text
                } else {
                    ""
                }
            } else if base_hidden {
                ""
            } else {
                &source[byte..next]
            };
            let start = out.visible.len();
            out.visible.push_str(text);
            let finish = out.visible.len();
            out.segs.push(Seg {
                src: byte..next,
                vis: start..finish,
            });
            if !text.is_empty() {
                if let Some(link) = &extra.link {
                    out.links.push((byte..next, link.clone()));
                }
                out.runs.push(RunAttrs {
                    len: text.len(),
                    marker,
                    dim,
                    style: inline,
                });
                extras.push(extra);
            }
        }
        (out, extras)
    }
}

pub(crate) fn fragment_plan(source: &str, reveal: bool) -> (RowPlan, Vec<Style>) {
    let root = parse_root(source);
    Document::parse(source, root.as_ref()).plan(source, 0, &Block::Paragraph, None, reveal, &[])
}

/// Transformed entities are indivisible source spans; never return a UTF-8 interior byte.
pub(crate) fn src_for_vis(segs: &[Seg], vis: usize) -> usize {
    for seg in segs {
        if !seg.vis.is_empty() && vis < seg.vis.end {
            return seg.src.start
                + if seg.src.len() == seg.vis.len() {
                    vis.saturating_sub(seg.vis.start)
                } else {
                    0
                };
        }
    }
    segs.last().map_or(0, |seg| seg.src.end)
}

pub(crate) fn vis_for_src(segs: &[Seg], src: usize) -> usize {
    for seg in segs {
        if src < seg.src.end {
            return seg.vis.start
                + if seg.src.len() == seg.vis.len() {
                    src.saturating_sub(seg.src.start)
                } else {
                    0
                };
        }
    }
    segs.last().map_or(0, |seg| seg.vis.end)
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
