use super::*;

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
    assert_eq!(fragment_plan("<!-- Badges -->", false).0.visible, "");
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
