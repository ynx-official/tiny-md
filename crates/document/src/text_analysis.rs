use crate::{Heading, outline};

#[derive(Default, Debug)]
pub struct AnalysisWork {
    pub counted_lines: usize,
    pub outlined: bool,
}

#[derive(Default)]
pub struct TextAnalysis {
    lines: Vec<String>,
    counts: Vec<usize>,
    characters: usize,
    headings: Vec<Heading>,
}

impl TextAnalysis {
    pub fn update(&mut self, text: &str) -> AnalysisWork {
        let new: Vec<_> = text.lines().collect();
        let prefix = self
            .lines
            .iter()
            .zip(&new)
            .take_while(|(a, b)| a.as_str() == **b)
            .count();
        let suffix = self.lines[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a.as_str() == **b)
            .count();
        let old_end = self.lines.len() - suffix;
        let new_end = new.len() - suffix;
        let outlined = self.lines.len() != new.len()
            || self.lines[prefix..old_end]
                .iter()
                .any(|line| affects_outline(line))
            || new[prefix..new_end]
                .iter()
                .any(|line| affects_outline(line));
        let counts: Vec<_> = new[prefix..new_end]
            .iter()
            .map(|line| line.chars().filter(|c| !c.is_whitespace()).count())
            .collect();
        self.characters -= self.counts[prefix..old_end].iter().sum::<usize>();
        self.characters += counts.iter().sum::<usize>();
        self.counts.splice(prefix..old_end, counts);
        self.lines.splice(
            prefix..old_end,
            new[prefix..new_end].iter().map(|line| (*line).to_owned()),
        );
        if outlined {
            self.headings = outline(text);
        }
        AnalysisWork {
            counted_lines: new_end - prefix,
            outlined,
        }
    }
    pub fn characters(&self) -> usize {
        self.characters
    }
    pub fn headings(&self) -> &[Heading] {
        &self.headings
    }
}

// Ordinary paragraph edits cannot change fence/front-matter context or ATX
// headings. Structural edits conservatively rebuild the outline so line offsets
// and navigation remain immediately correct, including after undo.
fn affects_outline(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with('#')
        || line.starts_with("```")
        || line.starts_with("~~~")
        || matches!(line, "---" | "...")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_edit_counts_only_changed_line_and_reuses_outline() {
        let mut cache = TextAnalysis::default();
        cache.update("# 标题\n中文 🌱\nunchanged");
        let work = cache.update("# 标题\n中文 🌱追加\nunchanged");
        assert_eq!(work.counted_lines, 1);
        assert!(!work.outlined);
        assert_eq!(cache.characters(), 17);
        let work = cache.update("# 标题\n中文 🌱追加\nunchanged");
        assert_eq!(work.counted_lines, 0);
        assert!(!work.outlined);
    }
    #[test]
    fn incremental_results_match_full_scans_across_structural_edits_and_undo() {
        let mut cache = TextAnalysis::default();
        let texts = [
            "",
            "# 标题\ntext",
            "# 标题\nchanged",
            "line\n# 标题\nchanged",
            "```\n# hidden\n```\n## visible",
            "text\n# hidden\n```\n## visible",
            "---\n# metadata\n...\n#正文\n# 正文",
            "---\n# metadata\ntext\n#正文\n# 正文",
            "# 普通\n~~~rust\n# hidden\n~~~~\n### last",
            "# 普通\n~~~rust\n# hidden\n~~~not-close\n### last",
            "# 普通\n~~~rust\n# hidden\n~~~~\n### last",
            "",
        ];
        for text in texts {
            cache.update(text);
            assert_eq!(cache.headings(), outline(text), "{text}");
            assert_eq!(
                cache.characters(),
                text.chars().filter(|c| !c.is_whitespace()).count(),
                "{text}"
            );
        }
    }
}
