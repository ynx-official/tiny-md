use super::*;
use core::prelude::v1::test;

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
}

#[gpui::test]
fn platform_styles_share_paragraph_spacing_across_theme_changes(cx: &mut TestAppContext) {
    let original = "# 标题\n\n第一段。\n\n第二段。";
    let first = "# 标题\n\n".encode_utf16().count();
    let blank = "# 标题\n\n第一段。\n".encode_utf16().count();
    let second = "# 标题\n\n第一段。\n\n".encode_utf16().count();

    for windows in [false, true] {
        cx.update(|cx| guise_theme(false).init(cx));
        let (editor, visual) = cx.add_window_view(|window, cx| {
            let editor = MarkdownEditor::new(cx)
                .value(original)
                .font_size(16.0)
                .style(editor_style_for_platform(false, windows));
            window.focus(&editor.focus_handle());
            editor
        });
        visual.simulate_resize(size(px(1000.0), px(700.0)));

        for dark in [false, true, false] {
            visual.update(|_, cx| {
                guise_theme(dark).init(cx);
                let style = editor_style_for_platform(dark, windows);
                assert_eq!(style.compact_headings, windows);
                editor.update(cx, |editor, cx| editor.set_style(style, cx));
            });
            draw(visual);
            draw(visual);
            visual.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    let mut bounds = |offset| {
                        editor
                            .bounds_for_range(offset..offset, Bounds::default(), window, cx)
                            .expect("rendered source line has input bounds")
                    };
                    let first = bounds(first);
                    let blank = bounds(blank);
                    let second = bounds(second);
                    assert_eq!(
                        blank.size.height,
                        px(13.0),
                        "single separator uses half a body line (windows={windows}, dark={dark})"
                    );
                    assert_eq!(second.origin.y - first.origin.y, px(39.0));
                    assert_eq!(first.size.height, px(26.0));
                    assert_eq!(second.size.height, px(26.0));
                    assert_eq!(editor.text(), original);
                });
            });
        }
    }
}
