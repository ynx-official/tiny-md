use super::*;

#[test]
fn functional_html_marks_keep_source_and_inline_break_behavior() {
    for source in [
        "<br/>",
        " <BR /> ",
        "<br>",
        "<!-- hello -->",
        "<!-- **中文** &amp; <br/> -->",
    ] {
        let plan = fragment_plan(source, false).0;
        assert_eq!(plan.visible, source);
        for (byte, _) in source.char_indices() {
            assert_eq!(vis_for_src(&plan.segs, byte), byte);
            assert_eq!(src_for_vis(&plan.segs, byte), byte);
        }
    }
    assert_eq!(
        fragment_plan("甲<br/>乙<br>丙", false).0.visible,
        "甲\n乙\n丙"
    );
    let source = "<!-- 多行\n    **文字** &amp; <br/>\n-->";
    let doc = Document::parse(source, parse_root(source).as_ref());
    for (line, text) in source.lines().enumerate() {
        let plan = doc
            .plan(text, doc.offsets[line], &Block::Paragraph, None, false, &[])
            .0;
        assert_eq!(plan.visible, text);
    }
}

#[test]
fn functional_html_marks_preserve_code_and_hidden_container_boundaries() {
    for source in [
        "`<br/>`",
        "`<!-- hello -->`",
        "\\<br/>",
        "\\<!-- hello -->",
        "<!-- 未完成",
    ] {
        let (_, styles) = fragment_plan(source, false);
        assert!(
            styles.iter().all(|style| style.annotation.is_none()),
            "{source}"
        );
    }
    let source = "<details>\n<summary>折叠</summary>\n<!-- hello -->\n<br/>\n</details>";
    let doc = Document::parse(source, parse_root(source).as_ref());
    for line in [2, 3] {
        let text = source.lines().nth(line).unwrap();
        assert_eq!(
            doc.plan(text, doc.offsets[line], &Block::Paragraph, None, false, &[])
                .0
                .visible,
            text
        );
        assert!(
            doc.plan(
                text,
                doc.offsets[line],
                &Block::Paragraph,
                None,
                false,
                &[doc.details[0].body.clone()]
            )
            .0
            .visible
            .is_empty()
        );
    }
    let source = "<table><tr><td>甲\n<br/>\n乙</td></tr></table>";
    let doc = Document::parse(source, parse_root(source).as_ref());
    let cell = &doc.tables[0].cells[0];
    let (plan, extra) = doc.plan(
        &source[cell.content.clone()],
        cell.content.start,
        &Block::Paragraph,
        None,
        false,
        &[],
    );
    assert!(!plan.visible.contains("<br"));
    assert!(extra.iter().all(|style| style.annotation.is_none()));
}

#[test]
fn nested_styles_entities_and_source_mapping() {
    let source = "中文 <strong><u>粗线</u></strong> &amp; &#x1f331;";
    let (p, extras) = fragment_plan(source, false);
    assert_eq!(p.visible, "中文 粗线 & 🌱");
    assert!(
        p.runs
            .iter()
            .zip(&extras)
            .any(|(r, e)| r.style.bold && e.underline)
    );
    for (byte, _) in p.visible.char_indices() {
        assert!(source.is_char_boundary(src_for_vis(&p.segs, byte)));
    }
    for (byte, _) in source.char_indices() {
        assert!(p.visible.is_char_boundary(vis_for_src(&p.segs, byte)));
    }
    assert_eq!(p.runs.iter().map(|r| r.len).sum::<usize>(), p.visible.len());
    assert_eq!(fragment_plan(source, true).0.visible, source);
}

#[test]
fn code_escapes_unknown_and_incomplete_tags_stay_literal() {
    for source in [
        "`<b>代码</b>`",
        "\\<b>文本",
        "<unknown>内容</unknown>",
        "<strong>正在输入",
        "<span title='未完成",
    ] {
        let p = fragment_plan(source, false).0;
        assert!(p.visible.contains('<'), "{source}: {}", p.visible);
    }
    assert_eq!(
        fragment_plan("<span title='a > b'>正文</span>", false)
            .0
            .visible,
        "正文"
    );
}

#[test]
fn details_table_spans_and_security_policy() {
    let source = "<details><summary>标题</summary><table><tr><th colspan='2'>头</th></tr><tr><td>A</td><td>B</td></tr></table></details>";
    let doc = Document::parse(source, parse_root(source).as_ref());
    assert_eq!(doc.details.len(), 1);
    assert_eq!(doc.tables.len(), 1);
    assert_eq!(doc.tables[0].cols, 2);
    assert_eq!(doc.tables[0].cells[0].colspan, 2);
    assert!(!doc.details[0].open);
    let (p, _) = fragment_plan(
        "<a href='jav&#x61;script:alert(1)' onclick='bad()'>链接</a>",
        false,
    );
    assert!(p.links.is_empty());
    assert_eq!(p.visible, "链接");
}

#[test]
fn block_html_keeps_markdown_literal_and_table_caption_is_not_lost() {
    assert_eq!(
        fragment_plan("<div>**原文**</div>", false).0.visible,
        "**原文**"
    );
    let source = "<table><caption>中文说明</caption><tr><td>A</td></tr></table>";
    let doc = Document::parse(source, parse_root(source).as_ref());
    assert!(
        doc.tables[0]
            .cells
            .iter()
            .any(|c| &source[c.content.clone()] == "中文说明")
    );
}

#[test]
fn multiline_styles_and_unsupported_tables_do_not_drop_content() {
    let source = "<span style='color:#c00;text-align:center'>\n中文\n</span>";
    let doc = Document::parse(source, parse_root(source).as_ref());
    let (p, extras) = doc.plan("中文", doc.offsets[1], &Block::Paragraph, None, false, &[]);
    assert_eq!(p.visible, "中文");
    assert_eq!(extras[0].color, Some(0xcc0000));
    assert_eq!(extras[0].align, Some(1));
    let raw = "<table><tr><td>正在输入</table>";
    assert_eq!(fragment_plan(raw, false).0.visible, raw);
}

#[test]
fn inline_html_code_does_not_disable_surrounding_markdown() {
    let (p, _) = fragment_plan("**粗体** 和 <code>**原文**</code>", false);
    assert_eq!(p.visible, "粗体 和 **原文**");
    assert!(p.runs[0].style.bold);
    assert!(p.runs.iter().any(|r| r.style.code));
}

#[test]
fn details_without_own_summary_has_a_default_label() {
    let source = "<details><details><summary>内层</summary>内容</details></details>";
    let doc = Document::parse(source, parse_root(source).as_ref());
    assert_eq!(doc.details.len(), 2);
    assert_eq!(doc.details[0].label, 0);
    assert_eq!(doc.details[0].body.start, "<details>".len());
    let (plan, _) = doc.plan(
        source,
        0,
        &Block::Paragraph,
        None,
        false,
        &[doc.details[0].body.clone()],
    );
    assert_eq!(plan.visible, "详细信息");
}

#[test]
fn oversized_table_falls_back_in_full_even_with_more_rows() {
    let source = format!(
        "<table><tr>{}</tr><tr><td>后续内容</td></tr></table>",
        "<td>值</td>".repeat(129)
    );
    let doc = Document::parse(&source, parse_root(&source).as_ref());
    assert!(doc.tables.is_empty());
    assert_eq!(
        doc.plan(&source, 0, &Block::Paragraph, None, false, &[])
            .0
            .visible,
        source
    );
}

#[test]
fn entity_immediately_before_closing_tag_has_an_exact_source_span() {
    for source in ["<b>中文 &amp;</b>", "<div>中文 &amp;</div>", "中文 &amp;"] {
        assert_eq!(fragment_plan(source, false).0.visible, "中文 &");
    }
    assert_eq!(
        fragment_plan("<div>&NotEqualTilde;中文&nbsp;&#65;</div>", false)
            .0
            .visible,
        "≂̸中文\u{a0}A"
    );
}

#[test]
fn nested_or_same_line_tables_keep_all_source_instead_of_partial_preview() {
    for source in [
        "<table><tr><td>外<table><tr><td>内</td></tr></table></td></tr></table>",
        "<table><tr><td>甲</td></tr></table><table><tr><td>乙</td></tr></table>",
    ] {
        let doc = Document::parse(source, parse_root(source).as_ref());
        assert!(doc.tables.is_empty());
        assert_eq!(
            doc.plan(source, 0, &Block::Paragraph, None, false, &[])
                .0
                .visible,
            source
        );
    }
}

#[test]
fn html_badge_groups_do_not_hide_mixed_text_or_unsupported_content() {
    for source in [
        "<p>说明 <a href='LICENSE'><img src='badge.svg'></a></p>",
        "<p><img src='badge.svg'><unknown>内容</unknown></p>",
        "<table><tr><td><p><img src='badge.svg'></p></td></tr></table>",
        "<p><img src='one.svg'></p><p><img src='two.svg'></p>",
    ] {
        let doc = Document::parse(source, parse_root(source).as_ref());
        assert!(doc.image_groups.is_empty(), "{source}");
    }
    let source = "<p align='center'><a href='javascript:alert(1)'><img src='badge.svg'></a><!-- gap --><img src='second.svg'></p>";
    let doc = Document::parse(source, parse_root(source).as_ref());
    assert_eq!(doc.image_groups.len(), 1);
    assert_eq!(doc.image_groups[0].align, 1);
    assert!(doc.link_at(source.find("<img").unwrap()).is_none());
    let (comment, style) = fragment_plan("<!-- Badges -->", false);
    assert_eq!(comment.visible, "<!-- Badges -->");
    assert_eq!(style[0].annotation, Some(Annotation::Comment));
    assert_eq!(
        fragment_plan("<!-- Badges -->", true).0.visible,
        "<!-- Badges -->"
    );
    assert!(
        fragment_plan("`<!-- Badges -->`", false)
            .0
            .visible
            .contains("<!-- Badges -->")
    );
}

#[test]
fn html_headings_keep_literal_markers_code_and_incomplete_tags() {
    let source = "<H3 id='title' style='color:#c00'><u>**标题**</u></H3>";
    let (plan, extra) = fragment_plan(source, false);
    assert_eq!(plan.visible, "**标题**");
    assert!(matches!(plan.kind, layout::RowKind::Heading(3)));
    assert!(
        extra
            .iter()
            .any(|style| style.underline && style.color == Some(0xcc0000))
    );
    assert_eq!(fragment_plan(source, true).0.visible, source);
    for source in [
        "<h3>正在输入",
        "<h3>错配</h2>",
        "`<h3>示例</h3>`",
        "\\<h3>转义</h3>",
    ] {
        let plan = fragment_plan(source, false).0;
        assert!(!matches!(plan.kind, layout::RowKind::Heading(_)));
        assert!(plan.visible.contains("<h3>"));
    }
}

#[test]
fn html_headings_keep_block_boundaries_and_body_text_size() {
    let (plan, _) = fragment_plan("<h3>甲</h3><h3>乙</h3>", false);
    assert_eq!(plan.visible, "甲\n乙");
    assert!(matches!(plan.kind, layout::RowKind::Heading(3)));
    let (plan, _) = fragment_plan("<h3>标题</h3>正文", false);
    assert_eq!(plan.visible, "标题\n正文");
    assert!(matches!(plan.kind, layout::RowKind::Paragraph));
    assert!(plan.runs.first().unwrap().style.bold);
    assert!(!plan.runs.last().unwrap().style.bold);
}
