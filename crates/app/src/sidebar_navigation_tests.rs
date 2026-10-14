use super::*;
use core::prelude::v1::test;

fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        guise_theme(false).init(cx);
        cx.set_global(Session::default());
    });
}

fn draw(cx: &mut VisualTestContext) {
    // The first frame lays out the sidebar; the second applies its reveal.
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
    }
}

fn write_note(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "# Note\n\nPreview").unwrap();
    std::fs::canonicalize(path).unwrap()
}

fn assert_current_visible(cx: &mut VisualTestContext, selector: &'static str) {
    let viewport = cx.debug_bounds("sidebar-content").unwrap();
    let row = cx.debug_bounds(selector).unwrap();
    assert!(
        row.top() >= viewport.top() && row.bottom() <= viewport.bottom(),
        "the current document must be fully visible: {row:?} inside {viewport:?}"
    );
}

#[gpui::test]
fn tree_defaults_to_only_the_current_document_ancestors(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let current = write_note(&root.join("work/nested/current.md"));
    write_note(&root.join("work/nested/child/hidden.md"));
    write_note(&root.join("work/sibling/hidden.md"));
    write_note(&root.join("other/hidden.md"));
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(Some(current.clone()), window, cx);
        view.library_root = Some(root.clone());
        view.show_sidebar(SidebarMode::Tree, cx);
        view.refresh_documents(cx);
        view
    });
    draw(cx);
    cx.read(|cx| {
        let view = view.read(cx);
        for open in [&root, &root.join("work"), &root.join("work/nested")] {
            assert!(
                !view.collapsed_folders.contains(open),
                "ancestor stays open"
            );
        }
        for closed in ["other", "work/sibling", "work/nested/child"] {
            assert!(
                view.collapsed_folders.contains(&root.join(closed)),
                "unrelated and descendant folders default to closed: {closed}"
            );
        }
        let visible = library::visible(&view.documents, "", &view.collapsed_folders, true);
        assert_eq!(visible.iter().filter(|entry| !entry.is_dir).count(), 1);
        assert!(visible.iter().any(|entry| entry.path == current));
    });
    cx.update(|_, cx| view.update(cx, |this, cx| this.toggle_folder(root.join("other"), cx)));
    draw(cx);
    cx.read(|cx| {
        assert!(
            !view
                .read(cx)
                .collapsed_folders
                .contains(&root.join("other"))
        )
    });
    for mode in [SidebarMode::Documents, SidebarMode::Tree] {
        cx.update(|_, cx| view.update(cx, |this, cx| this.show_sidebar(mode, cx)));
        draw(cx);
    }
    cx.read(|cx| {
        assert!(
            view.read(cx)
                .collapsed_folders
                .contains(&root.join("other"))
        )
    });
}

#[gpui::test]
fn list_and_tree_reveal_the_current_document_on_mode_switch(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    for index in 0..40 {
        write_note(&root.join(format!("note-{index:02}.md")));
    }
    let current = write_note(&root.join("z-current.md"));
    let (view, cx) =
        cx.add_window_view(|window, cx| TinyMd::new(Some(current.clone()), window, cx));
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    cx.update(|_, cx| view.update(cx, |this, cx| this.show_sidebar(SidebarMode::Documents, cx)));
    draw(cx);
    assert_current_visible(cx, "document-entry-40");
    // An ordinary repaint must allow the user to browse away from the note.
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.sidebar_scroll[SidebarMode::Documents as usize]
                .set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        })
    });
    draw(cx);
    assert_eq!(
        cx.read(|cx| view.read(cx).sidebar_scroll[0].offset().y),
        px(0.0)
    );
    for (mode, selector) in [
        (SidebarMode::Tree, "document-entry-41"),
        (SidebarMode::Documents, "document-entry-40"),
    ] {
        cx.update(|_, cx| view.update(cx, |this, cx| this.show_sidebar(mode, cx)));
        draw(cx);
        assert_current_visible(cx, selector);
        cx.update(|_, cx| {
            view.update(cx, |this, cx| {
                this.sidebar_scroll[mode as usize].set_offset(point(px(0.0), px(0.0)));
                cx.notify();
            })
        });
        draw(cx);
        cx.dispatch_action(ToggleSidebar);
        draw(cx);
        cx.read(|cx| assert!(!view.read(cx).sidebar));
        cx.dispatch_action(ToggleSidebar);
        draw(cx);
        assert_current_visible(cx, selector);
    }
}

#[gpui::test]
fn initially_visible_sidebar_reveals_the_document_after_loading(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    for index in 0..40 {
        write_note(&directory.path().join(format!("note-{index:02}.md")));
    }
    let current = write_note(&directory.path().join("z-current.md"));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(Some(current.clone()), window, cx);
        view.sidebar = true;
        view
    });
    draw(cx);
    assert_current_visible(cx, "document-entry-40");
}

#[gpui::test]
fn switching_documents_closes_the_old_branch_and_reveals_the_new_one(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let first = write_note(&root.join("a-first/current.md"));
    let second = write_note(&root.join("z-second/nested/current.md"));
    for index in 0..40 {
        write_note(&root.join(format!("folder-{index:02}/hidden.md")));
    }
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(Some(first.clone()), window, cx);
        view.library_root = Some(root.clone());
        view.show_sidebar(SidebarMode::Tree, cx);
        view.refresh_documents(cx);
        view
    });
    cx.simulate_resize(size(px(1000.0), px(500.0)));
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.request(Intent::SwitchPath(second.clone()), window, cx);
        })
    });
    draw(cx);
    cx.read(|cx| {
        let view = view.read(cx);
        assert_eq!(view.document.path(), Some(second.as_path()));
        assert!(view.collapsed_folders.contains(&root.join("a-first")));
        assert!(!view.collapsed_folders.contains(&root.join("z-second")));
        assert!(
            !view
                .collapsed_folders
                .contains(&root.join("z-second/nested"))
        );
    });
    assert_current_visible(cx, "document-entry-44");
}

#[gpui::test]
fn untitled_tree_defaults_to_all_directories_closed(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    write_note(&root.join("nested/hidden.md"));
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.library_root = Some(root.clone());
        view.show_sidebar(SidebarMode::Tree, cx);
        view.refresh_documents(cx);
        view
    });
    draw(cx);
    cx.read(|cx| {
        let view = view.read(cx);
        assert!(view.collapsed_folders.contains(&root));
        assert!(view.collapsed_folders.contains(&root.join("nested")));
        assert!(library::visible(&view.documents, "", &view.collapsed_folders, true).is_empty());
    });
}
