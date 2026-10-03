//! Stateful fence highlighting, cached independently of layout and theme.
use std::{collections::HashMap, ops::Range, sync::LazyLock};

use guise::editor::{Highlighter, Language, LineState, TokenKind};
use syntect::{
    easy::ScopeRangeIterator,
    parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
};

use crate::code_blocks::CodeBlock;

type Tokens = Vec<(Range<usize>, TokenKind)>;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static SCOPES: LazyLock<Vec<(Scope, TokenKind)>> = LazyLock::new(|| {
    [
        ("comment", TokenKind::Comment),
        ("string", TokenKind::StringLit),
        ("constant.numeric", TokenKind::Number),
        ("constant.language", TokenKind::Keyword),
        ("keyword", TokenKind::Keyword),
        ("storage", TokenKind::Keyword),
        ("entity.name.function", TokenKind::Function),
        ("support.function", TokenKind::Function),
        ("entity.name.tag", TokenKind::Type),
        ("entity.name.type", TokenKind::Type),
        ("entity.name.class", TokenKind::Type),
        ("support.type", TokenKind::Type),
        ("support.class", TokenKind::Type),
        ("entity.other.attribute-name", TokenKind::Function),
    ]
    .map(|(scope, kind)| (Scope::new(scope).expect("valid built-in scope"), kind))
    .to_vec()
});

struct Entry {
    language: String,
    source: String,
    lines: Vec<Tokens>,
}

#[derive(Default)]
pub(crate) struct Cache {
    entries: HashMap<usize, Entry>,
}

impl Cache {
    pub fn update(&mut self, blocks: &[CodeBlock]) {
        self.entries
            .retain(|start, _| blocks.iter().any(|block| block.start == *start));
        for block in blocks {
            if self.entries.get(&block.start).is_some_and(|entry| {
                entry.language == block.language && entry.source == block.source
            }) {
                continue;
            }
            self.entries.insert(
                block.start,
                Entry {
                    language: block.language.clone(),
                    source: block.source.clone(),
                    lines: highlight(&block.language, &block.source),
                },
            );
        }
    }

    pub fn line(&self, start: usize, offset: usize) -> &[(Range<usize>, TokenKind)] {
        self.entries
            .get(&start)
            .and_then(|entry| entry.lines.get(offset))
            .map_or(&[], Vec::as_slice)
    }
}

fn tag(language: &str) -> String {
    language
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn grammar(language: &str) -> Option<&'static SyntaxReference> {
    let language = tag(language);
    let token = match language.as_str() {
        "" | "text" | "txt" | "plaintext" | "plain" | "mermaid" => return None,
        "rust" => "rs",
        "python" => "py",
        "javascript" | "node" | "nodejs" => "js",
        "typescript" => "ts",
        "golang" => "go",
        "c++" | "cxx" | "cc" | "hpp" => "cpp",
        "shell" | "shellscript" | "bash" | "zsh" | "console" => "sh",
        "yml" => "yaml",
        "htm" => "html",
        "jsonc" => "json",
        "md" => "markdown",
        "rb" => "ruby",
        "cs" | "csharp" => "cs",
        other => other,
    };
    SYNTAXES.find_syntax_by_token(token)
}

fn kind(stack: &ScopeStack) -> Option<TokenKind> {
    // An outer comment/string wins over punctuation or keywords nested in it.
    SCOPES.iter().find_map(|(prefix, kind)| {
        stack
            .scopes
            .iter()
            .any(|scope| prefix.is_prefix_of(*scope))
            .then_some(*kind)
    })
}

fn highlight(language: &str, source: &str) -> Vec<Tokens> {
    let Some(syntax) = grammar(language) else {
        // Guise also supplies TypeScript and TOML, absent from some syntax sets.
        let language = fence_language(Some(language));
        let mut state = LineState::default();
        return source
            .split('\n')
            .map(|line| language.line(line, &mut state))
            .collect();
    };
    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    source
        .split('\n')
        .map(|line| {
            // Grammars see newlines, while GPUI ranges cover only source bytes.
            let input = format!("{line}\n");
            let Ok(ops) = parser.parse_line(&input, &SYNTAXES) else {
                parser = ParseState::new(syntax);
                stack = ScopeStack::new();
                return Vec::new();
            };
            let mut tokens = Vec::new();
            for (range, op) in ScopeRangeIterator::new(&ops, &input) {
                if stack.apply(op).is_err() {
                    stack = ScopeStack::new();
                    continue;
                }
                let range = range.start.min(line.len())..range.end.min(line.len());
                if !range.is_empty()
                    && let Some(kind) = kind(&stack)
                {
                    tokens.push((range, kind));
                }
            }
            tokens
        })
        .collect()
}

/// Guise fallback for source Markdown and grammars missing from Syntect.
pub(crate) fn fence_language(language: Option<&str>) -> Language {
    match tag(language.unwrap_or("")).as_str() {
        "rust" | "rs" => Language::Rust,
        "sql" => Language::Sql,
        "json" | "jsonc" => Language::Json,
        "toml" => Language::Toml,
        "python" | "py" => Language::Python,
        "javascript" | "js" | "jsx" => Language::JavaScript,
        "typescript" | "ts" | "tsx" => Language::TypeScript,
        "go" | "golang" => Language::Go,
        "c" | "h" | "cpp" | "c++" | "cxx" | "hpp" => Language::C,
        "markdown" | "md" => Language::Markdown,
        _ => Language::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_at(tokens: &Tokens, offset: usize) -> Option<TokenKind> {
        tokens
            .iter()
            .find_map(|(range, kind)| range.contains(&offset).then_some(*kind))
    }

    #[test]
    fn common_languages_and_aliases_have_highlights() {
        for (language, source) in [
            ("rust", "fn main() { let n = 42; }"),
            ("PY example", "def greet(): return \"你好\""),
            ("javascript", "const message = \"你好\";"),
            ("TS title", "const message: string = \"你好\";"),
            ("go", "func main() { println(42) }"),
            ("c++", "int main() { return 42; }"),
            ("java", "public class Main { int n = 42; }"),
            ("sh", "echo \"你好\" # comment"),
            ("bash", "echo \"你好\" # comment"),
            ("yaml", "message: \"你好\" # comment"),
            ("html", "<div class=\"card\">你好</div>"),
            ("css", ".card { color: red; width: 42px; }"),
            ("sql", "SELECT * FROM notes WHERE id = 42;"),
            ("json", "{\"message\": \"你好\", \"count\": 42}"),
            ("toml", "message = \"你好\""),
        ] {
            let lines = highlight(language, source);
            assert!(!lines[0].is_empty(), "no highlights for {language}");
            for (range, _) in &lines[0] {
                assert!(source.is_char_boundary(range.start));
                assert!(source.is_char_boundary(range.end));
                assert!(range.end <= source.len());
            }
        }
        for language in ["java", "sh", "bash", "yaml", "html", "css", "cpp"] {
            assert!(grammar(language).is_some(), "missing grammar: {language}");
        }
    }

    #[test]
    fn comments_and_strings_keep_state_across_blank_lines() {
        let lines = highlight("java", "/* 注释\n\nreturn 42;\n*/ int n = 1;");
        assert_eq!(token_at(&lines[2], 0), Some(TokenKind::Comment));
        assert_eq!(token_at(&lines[3], 11), Some(TokenKind::Number));
        let lines = highlight(
            "python",
            "message = \"\"\"你好\n\nreturn 42\n\"\"\"\ncount = 42",
        );
        assert_eq!(token_at(&lines[2], 0), Some(TokenKind::StringLit));
        assert_eq!(token_at(&lines[4], 8), Some(TokenKind::Number));
    }

    #[test]
    fn cache_updates_edits_and_language_and_isolates_fences() {
        let mut cache = Cache::default();
        let lines = "```java\n/* open\n```\n```java\nint n = 42;\n```\n```python\n# 注释";
        let (mut blocks, _) =
            crate::code_blocks::collect(&lines.split('\n').map(str::to_owned).collect::<Vec<_>>());
        cache.update(&blocks);
        assert_eq!(cache.line(3, 0).last().unwrap().1, TokenKind::Number);
        assert_eq!(cache.line(6, 0)[0].1, TokenKind::Comment);
        let original = cache.line(3, 0).as_ptr();
        cache.update(&blocks);
        assert_eq!(cache.line(3, 0).as_ptr(), original);
        blocks[2].language = "text".into();
        cache.update(&blocks);
        assert!(cache.line(6, 0).is_empty());
        blocks[1].source = "// 修改".into();
        cache.update(&blocks);
        assert_eq!(cache.line(3, 0)[0].1, TokenKind::Comment);
        cache.update(&[]);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn unknown_and_unlabelled_fences_stay_plain() {
        for language in ["", "unknown-language", "text", "plaintext", "mermaid"] {
            assert!(highlight(language, "fn main() { return 42; }")[0].is_empty());
        }
        assert_eq!(fence_language(Some("TS title")), Language::TypeScript);
        assert_eq!(fence_language(None), Language::None);
    }
}
