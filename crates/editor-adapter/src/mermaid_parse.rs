//! Compatibility fixes remain local; saved source and cache keys stay intact.
use mermaid_rs_renderer::{ParseOutput, parse_mermaid_strict};

pub(crate) fn parse(source: &str) -> Result<ParseOutput, String> {
    let validation_source = protect_end_ids_for_validation(source);
    let normalized = normalize_pipe_chains(source);
    let mut parsed = if validation_source.is_some() || normalized.is_some() {
        // 0.3.1's balance preflight lowercases whole lines, while its actual
        // flowchart parser correctly treats only a bare lowercase `end` as a
        // closer. Fix only the validation copy. Pipe-labelled chains are
        // expanded into equivalent same-line statements for the parser;
        // IDs, labels, styles, links, saved source and cache keys stay intact.
        mermaid_rs_renderer::validator::validate(validation_source.as_deref().unwrap_or(source))
            .map_err(|error| error.to_string())?;
        check_node_brackets(source)?;
        mermaid_rs_renderer::parse_mermaid(normalized.as_deref().unwrap_or(source))
            .map_err(|error| error.to_string())?
    } else {
        parse_mermaid_strict(source).map_err(|error| error.to_string())?
    };
    if parsed.graph.kind == mermaid_rs_renderer::DiagramKind::Flowchart {
        // The pinned parser retains double-quote delimiters on pipe labels.
        // Strip one matching pair in the IR, leaving source, inner quotes and
        // entity escapes intact, just as node-label delimiters are omitted.
        for edge in &mut parsed.graph.edges {
            if let Some(text) = edge.label.as_deref()
                && let Some(text) = text
                    .strip_prefix('"')
                    .and_then(|text| text.strip_suffix('"'))
            {
                edge.label = Some(text.to_owned());
            }
        }
    }
    Ok(parsed)
}

fn normalize_pipe_chains(source: &str) -> Option<String> {
    // Match the pinned parser's arrow grammar, outside labels/quoted content.
    static ARROW: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"<[-.=ox]*[-=]+[-.=ox]*>|<[-.=ox]*[-=]+[-.=ox]*|[-.=ox]*[-=]+[-.=ox]*>|[-.=ox]*[-=]+[-.=ox]*").unwrap()
    });
    let mut frontmatter = false;
    let mut header_seen = false;
    let mut changed = false;
    let mut result = String::with_capacity(source.len());
    for raw in source.split_inclusive('\n') {
        let trimmed = raw.trim();
        if !header_seen {
            if trimmed == "---" {
                frontmatter = !frontmatter;
            } else if !frontmatter && !trimmed.is_empty() && !trimmed.starts_with("%%") {
                let header = trimmed.split_whitespace().next().unwrap_or_default();
                if !header.eq_ignore_ascii_case("flowchart")
                    && !header.eq_ignore_ascii_case("graph")
                {
                    return None;
                }
                header_seen = true;
            }
            result.push_str(raw);
            continue;
        }
        if !raw.contains('|') {
            result.push_str(raw);
            continue;
        }
        let masked = mask_labels(raw);
        let boundaries = std::iter::once(0)
            .chain(
                masked
                    .bytes()
                    .enumerate()
                    .filter(|(_, byte)| *byte == b';')
                    .map(|(index, _)| index + 1),
            )
            .chain(std::iter::once(raw.len()))
            .collect::<Vec<_>>();
        for span in boundaries.windows(2) {
            let (start, end) = (span[0], span[1]);
            let statement = &raw[start..end];
            let matches = ARROW.find_iter(&masked[start..end]).collect::<Vec<_>>();
            if matches.len() < 2 {
                result.push_str(statement);
                continue;
            }
            let mut nodes = Vec::new();
            let mut arrows = Vec::new();
            let mut previous = 0;
            let mut labelled = false;
            for arrow in matches {
                nodes.push(statement[previous..arrow.start()].trim());
                let mut tail = arrow.end();
                let after = statement[tail..].trim_start();
                if let Some(label) = after.strip_prefix('|')
                    && let Some(close) = label.find('|')
                {
                    tail = statement.len() - after.len() + close + 2;
                    labelled = true;
                }
                arrows.push(&statement[arrow.start()..tail]);
                previous = tail;
            }
            nodes.push(statement[previous..].trim().trim_end_matches(';').trim());
            if !labelled || nodes.iter().any(|node| node.is_empty()) {
                result.push_str(statement);
                continue;
            }
            for index in 0..arrows.len() {
                if index > 0 {
                    result.push_str("; ");
                }
                result.push_str(&format!(
                    "{} {} {}",
                    nodes[index],
                    arrows[index],
                    nodes[index + 1]
                ));
            }
            if statement.trim_end().ends_with(';') {
                result.push(';');
            }
            if statement.ends_with('\n') {
                result.push('\n');
            }
            changed = true;
        }
    }
    changed.then_some(result)
}

fn mask_labels(source: &str) -> String {
    let mut masked = source.as_bytes().to_vec();
    let (mut depth, mut quote, mut escaped, mut pipe) = (0usize, None, false, false);
    let mut previous = b' ';
    for (index, byte) in source.bytes().enumerate() {
        if quote.is_none()
            && depth == 0
            && !pipe
            && byte == b'%'
            && source.as_bytes().get(index + 1) == Some(&b'%')
        {
            masked[index..].fill(b' ');
            break;
        }
        let hidden = depth > 0 || quote.is_some() || pipe;
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == delimiter {
                quote = None;
            }
        } else if starts_quote(byte, previous) {
            quote = Some(byte);
        } else if depth == 0 && byte == b'|' {
            pipe = !pipe;
        } else if !pipe {
            match byte {
                b'[' | b'(' | b'{' => depth += 1,
                b']' | b')' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if hidden || depth > 0 || quote.is_some() || pipe || byte == b'|' {
            masked[index] = b' ';
        }
        previous = byte;
    }
    String::from_utf8(masked).expect("complete label byte ranges are masked")
}

fn starts_quote(byte: u8, previous: u8) -> bool {
    byte == b'"'
        || (byte == b'\''
            && (previous.is_ascii_whitespace()
                || matches!(previous, b'[' | b'(' | b'{' | b'=' | b':' | b'|')))
}

fn check_node_brackets(source: &str) -> Result<(), String> {
    let mut stack = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut header_seen = false;
    let mut in_frontmatter = false;
    for (line, raw) in source.lines().enumerate() {
        if !header_seen {
            if raw.trim() == "---" {
                in_frontmatter = !in_frontmatter;
            }
            if in_frontmatter {
                continue;
            }
            let header = raw.split_whitespace().next().unwrap_or_default();
            header_seen =
                header.eq_ignore_ascii_case("flowchart") || header.eq_ignore_ascii_case("graph");
            continue;
        }
        let mut bytes = raw.bytes().peekable();
        let mut previous = b' ';
        while let Some(byte) = bytes.next() {
            if let Some(delimiter) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == delimiter {
                    quote = None;
                }
                previous = byte;
                continue;
            }
            if byte == b'%' && bytes.peek() == Some(&b'%') {
                break;
            }
            if starts_quote(byte, previous) {
                quote = Some(byte);
            } else {
                match byte {
                    b'[' | b'(' | b'{' => stack.push((byte, line + 1)),
                    b']' | b')' | b'}' => {
                        let open = match byte {
                            b']' => b'[',
                            b')' => b'(',
                            _ => b'{',
                        };
                        if stack.pop().is_none_or(|(found, _)| found != open) {
                            return Err(format!("流程图节点括号不匹配（第 {} 行）。", line + 1));
                        }
                    }
                    _ => {}
                }
            }
            previous = byte;
        }
    }
    if let Some((_, line)) = stack.first() {
        return Err(format!("流程图节点括号未闭合（第 {line} 行）。"));
    }
    Ok(())
}

fn protect_end_ids_for_validation(source: &str) -> Option<String> {
    let mut in_frontmatter = false;
    let mut header_seen = false;
    let mut protected: Option<Vec<u8>> = None;
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let trimmed = raw.trim_start();
        if !header_seen {
            if trimmed.trim() == "---" {
                in_frontmatter = !in_frontmatter;
            } else if !in_frontmatter && !trimmed.is_empty() && !trimmed.starts_with("%%") {
                let header = trimmed.split_whitespace().next().unwrap_or_default();
                if !header.eq_ignore_ascii_case("flowchart")
                    && !header.eq_ignore_ascii_case("graph")
                {
                    return None;
                }
                header_seen = true;
            }
        } else if let Some(id) = trimmed.get(..3)
            && id != "end"
            && id.eq_ignore_ascii_case("end")
        {
            let rest = &trimmed[3..];
            if rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with("%%") {
                let capital = id
                    .bytes()
                    .position(|byte| byte.is_ascii_uppercase())
                    .unwrap();
                let position = offset + raw.len() - trimmed.len() + capital;
                // Same byte width preserves every upstream line/column number.
                protected.get_or_insert_with(|| source.as_bytes().to_vec())[position] = b'_';
            }
        }
        offset += raw.len();
    }
    protected.map(|bytes| String::from_utf8(bytes).expect("only ASCII ID bytes were replaced"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_flowchart_edge_labels_display_the_text_without_syntax_quotes() {
        let parsed = parse(
            "flowchart TD\nA -->|\"Go / Conditional Go\"| B\nB -->|\"含 \\\"引号\\\" 的文字\"| C -->|继续| D\nD -->|\"#quot;原样引号#quot;\"| E",
        )
        .unwrap();
        assert_eq!(parsed.graph.edges.len(), 4);
        assert_eq!(
            parsed.graph.edges[0].label.as_deref(),
            Some("Go / Conditional Go")
        );
        assert_eq!(
            parsed.graph.edges[1].label.as_deref(),
            Some("含 \\\"引号\\\" 的文字")
        );
        assert_eq!(parsed.graph.edges[2].label.as_deref(), Some("继续"));
        assert_eq!(
            parsed.graph.edges[3].label.as_deref(),
            Some("#quot;原样引号#quot;")
        );
    }

    #[test]
    fn compatibility_keeps_frontmatter_literal_brackets_and_single_quoted_labels() {
        let parsed = parse(
            "---\ndescription: |\n  graph TD\n  [literal metadata\n---\nflowchart TD\nEND --> A",
        )
        .unwrap();
        assert_eq!(parsed.graph.nodes.len(), 2);
        let parsed =
            parse("flowchart TD\nEND -->|继续| B['literal [ ; -->'] --> C[O'Reilly]").unwrap();
        assert_eq!(parsed.graph.nodes.len(), 3);
        assert_eq!(parsed.graph.edges.len(), 2);
        assert_eq!(parsed.graph.nodes["B"].label, "literal [ ; -->");
        assert_eq!(parsed.graph.nodes["C"].label, "O'Reilly");
    }

    #[test]
    fn labelled_chains_keep_all_edges_and_ignore_arrows_inside_labels() {
        let parsed =
            parse("flowchart TD\nA -->|允许 --> 办理| B[解释 --> 测试] -->|继续| C[含|管道]")
                .unwrap();
        assert_eq!(parsed.graph.nodes.len(), 3);
        assert_eq!(parsed.graph.edges.len(), 2);
        assert_eq!(parsed.graph.edges[0].to, "B");
        assert_eq!(
            parsed.graph.edges[0].label.as_deref(),
            Some("允许 --> 办理")
        );
        assert_eq!(parsed.graph.edges[1].from, "B");
        assert_eq!(parsed.graph.edges[1].label.as_deref(), Some("继续"));
        assert_eq!(parsed.graph.nodes["B"].label, "解释 --> 测试");
        assert_eq!(parsed.graph.nodes["C"].label, "含|管道");
    }

    #[test]
    fn labelled_chains_keep_semicolons_quotes_comments_and_subgraph_membership() {
        let parsed = parse("flowchart TD\nsubgraph G\nA -->|first| B[\"文本; [中间]\"] --> C; C -->|later| D --> E %% don't -->\nend").unwrap();
        assert_eq!(parsed.graph.nodes.len(), 5);
        assert_eq!(parsed.graph.edges.len(), 4);
        assert_eq!(parsed.graph.edges[2].label.as_deref(), Some("later"));
        assert_eq!(parsed.graph.nodes["B"].label, "文本; [中间]");
        assert_eq!(parsed.graph.subgraphs[0].nodes.len(), 5);
    }

    #[test]
    fn capitalized_end_ids_keep_their_names_labels_and_links() {
        for id in ["END", "End", "eNd", "enD"] {
            let source = format!(
                "flowchart TD\nA --> {id}[END 与 end 原样显示]\n{id} --> B\nstyle {id} fill:#123456\nclick {id} \"https://example.com/END\""
            );
            let parsed = parse(&source).unwrap();
            assert_eq!(parsed.graph.nodes.len(), 3);
            assert_eq!(parsed.graph.edges.len(), 2);
            assert_eq!(parsed.graph.nodes[id].label, "END 与 end 原样显示");
            assert_eq!(parsed.graph.edges[1].from, id);
            assert_eq!(
                parsed.graph.node_styles[id].fill.as_deref(),
                Some("#123456")
            );
            assert_eq!(parsed.graph.node_links[id].url, "https://example.com/END");
        }
    }

    #[test]
    fn capitalized_end_nodes_do_not_close_real_subgraphs() {
        let parsed = parse("%% comment\n%%{init: {\"flowchart\": {\"defaultRenderer\": \"elk\"}}}%%\nflowchart TD\nsubgraph G[阶段]\nEND\nEND --> A\nend %% close\nA --> B").unwrap();
        assert_eq!(parsed.graph.subgraphs.len(), 1);
        assert!(parsed.graph.subgraphs[0].nodes.contains(&"END".into()));
        assert!(parsed.graph.subgraphs[0].nodes.contains(&"A".into()));
        assert!(!parsed.graph.subgraphs[0].nodes.contains(&"B".into()));
        assert_eq!(
            parsed.init_config.unwrap()["flowchart"]["defaultRenderer"],
            "elk"
        );
    }

    #[test]
    fn malformed_flowcharts_still_fail_with_capitalized_end_ids() {
        for source in [
            "flowchart TD\nEND --> A\nend",
            "flowchart TD\nsubgraph G\nEND --> A",
            "flowchart TD\nEND --> A[unclosed",
            "flowchart TD\nEND --> A\nB -->",
        ] {
            assert!(parse(source).is_err(), "must reject {source}");
        }
    }

    #[test]
    fn other_diagram_types_retain_the_upstream_parser() {
        for source in [
            "sequenceDiagram\nalt ready\nAlice->>Bob: END\nend",
            "sequenceDiagram\nAlice->>Bob: Hello\nEND --> Bob",
            "classDiagram\nclass END\nEND --> A",
        ] {
            let expected = parse_mermaid_strict(source);
            let actual = parse(source);
            assert_eq!(actual.is_ok(), expected.is_ok());
            if let (Ok(actual), Ok(expected)) = (actual, expected) {
                assert_eq!(
                    actual.graph.nodes.keys().collect::<Vec<_>>(),
                    expected.graph.nodes.keys().collect::<Vec<_>>()
                );
                assert_eq!(actual.graph.edges.len(), expected.graph.edges.len());
            }
        }
    }
}
