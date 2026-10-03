//! Fence metadata shares Guise's classifier with the editable source layout.
use guise::markdown::block::{Block, DocState, classify};

pub struct CodeBlock {
    pub start: usize,
    pub end: usize,
    pub closed: bool,
    pub language: String,
    pub source: String,
}

impl CodeBlock {
    pub fn is_mermaid(&self) -> bool {
        self.language.eq_ignore_ascii_case("mermaid")
    }

    pub fn touches(&self, range: Option<(usize, usize)>) -> bool {
        range.is_some_and(|(start, end)| start <= self.end && end >= self.start)
    }
}

pub fn collect(lines: &[String]) -> (Vec<CodeBlock>, Vec<Option<usize>>) {
    let mut state = DocState::default();
    let mut blocks = Vec::new();
    let mut active = None;
    let mut membership = vec![None; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        match classify(line, &mut state) {
            Block::Fence { open: true, lang } => {
                let index = blocks.len();
                blocks.push(CodeBlock {
                    start: i,
                    end: lines.len() - 1,
                    closed: false,
                    language: lang
                        .unwrap_or_default()
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_owned(),
                    source: String::new(),
                });
                active = Some(index);
                membership[i] = active;
            }
            Block::Fence { open: false, .. } => {
                membership[i] = active;
                if let Some(index) = active.take() {
                    blocks[index].end = i;
                    blocks[index].closed = true;
                }
            }
            Block::CodeLine => {
                membership[i] = active;
                if let Some(index) = active {
                    if i > blocks[index].start + 1 {
                        blocks[index].source.push('\n');
                    }
                    blocks[index].source.push_str(line);
                }
            }
            _ => {}
        }
    }
    (blocks, membership)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_share_the_editor_classifier_and_preserve_source() {
        let lines = "---\nname: x\n---\n~~~~rust example\n```\n你好\n~~~~\n\n```mermaid\nflowchart LR\nA-->B".lines().map(str::to_owned).collect::<Vec<_>>();
        let (blocks, map) = collect(&lines);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].source, "```\n你好");
        assert_eq!(blocks[0].language, "rust");
        assert!(blocks[0].closed);
        assert!(blocks[1].is_mermaid());
        assert!(!blocks[1].closed);
        assert_eq!(map[2], None);
        assert_eq!(map[4], Some(0));
        assert_eq!(map[10], Some(1));
        assert!(blocks[1].touches(Some((7, 9))));
        assert!(!blocks[1].touches(Some((0, 7))));
    }
}
