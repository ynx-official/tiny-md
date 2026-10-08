use super::*;
use core::prelude::v1::test;

pub(super) fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        guise_theme(false).init(cx);
        cx.set_global(Session::default());
    });
}

pub(super) fn note(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::canonicalize(path).unwrap()
}

pub(super) fn assert_opened(cx: &App, path: &Path, text: &str) {
    assert_eq!(
        cx.windows().len(),
        2,
        "opening a note creates another window"
    );
    let (handle, view) = cx.global::<Session>().windows.last().unwrap();
    assert!(cx.windows().contains(handle));
    let view = view.upgrade().unwrap();
    let view = view.read(cx);
    assert_eq!(view.document.path(), Some(path));
    assert_eq!(view.editor.read(cx).text(), text);
    assert!(!view.dirty);
}

#[gpui::test]
fn opening_another_note_preserves_the_current_document(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let first = note(directory.path(), "first.md", "# First note\n");
    let second = note(directory.path(), "second.md", "# Second note\n");
    let (view, cx) = cx.add_window_view(|window, cx| TinyMd::new(Some(first.clone()), window, cx));
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.request(Intent::OpenPath(second.clone()), window, cx);
        });
    });
    cx.run_until_parked();
    cx.read(|cx| {
        assert_opened(cx, &second, "# Second note\n");
        assert_eq!(view.read(cx).document.path(), Some(first.as_path()));
        assert_eq!(view.read(cx).editor.read(cx).text(), "# First note\n");
    });
}

#[gpui::test]
fn opening_a_note_keeps_unsaved_text_without_a_save_prompt(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let second = note(directory.path(), "second.md", "Second note");
    let (view, cx) = cx.add_window_view(|window, cx| TinyMd::new(None, window, cx));
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.install(Document::untitled("base"), "base", window, cx);
            this.editor.update(cx, |editor, cx| {
                editor.command(EditorCommand::Wrap("**"), cx);
            });
            this.request(Intent::OpenPath(second.clone()), window, cx);
        });
    });
    assert!(
        !cx.has_pending_prompt(),
        "opening must not discard or save the old note"
    );
    cx.run_until_parked();
    cx.read(|cx| {
        assert_opened(cx, &second, "Second note");
        assert_eq!(view.read(cx).document.path(), None);
        assert_eq!(view.read(cx).editor.read(cx).text(), "**base**");
        assert!(view.read(cx).dirty);
        assert!(!view.read(cx).busy);
    });
    cx.update(|window, cx| {
        view.read(cx).editor.read(cx).focus_handle().focus(window);
    });
    cx.dispatch_action(guise::actions::Undo);
    assert_eq!(cx.read(|cx| view.read(cx).editor.read(cx).text()), "base");
}

#[gpui::test]
fn opening_a_recent_note_creates_a_window(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let first = note(directory.path(), "first.md", "First note");
    let second = note(directory.path(), "second.md", "Second note");
    let (view, cx) = cx.add_window_view(|window, cx| TinyMd::new(Some(first.clone()), window, cx));
    cx.update(|window, cx| {
        remember(&second, cx);
        view.update(cx, |this, cx| this.recent(0, window, cx));
    });
    cx.run_until_parked();
    cx.read(|cx| {
        assert_opened(cx, &second, "Second note");
        assert_eq!(view.read(cx).document.path(), Some(first.as_path()));
    });
}

#[gpui::test]
fn opening_multiple_notes_keeps_successes_when_one_file_is_missing(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let first = note(directory.path(), "first.md", "First note");
    let second = note(directory.path(), "second.md", "Second note");
    let missing = directory.path().join("missing.md");
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(
            Document::untitled("Current note"),
            "Current note",
            window,
            cx,
        );
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.open_paths(vec![first.clone(), missing, second.clone()], window, cx);
        });
    });
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(cx.windows().len(), 3);
        assert_eq!(view.read(cx).editor.read(cx).text(), "Current note");
        assert!(view.read(cx).error);
        assert!(view.read(cx).status.contains("missing.md"));
        assert!(!view.read(cx).busy);
        let opened = cx
            .global::<Session>()
            .windows
            .iter()
            .map(|(_, view)| view.upgrade().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(opened[0].read(cx).document.path(), Some(first.as_path()));
        assert_eq!(opened[0].read(cx).editor.read(cx).text(), "First note");
        assert_eq!(opened[1].read(cx).document.path(), Some(second.as_path()));
        assert_eq!(opened[1].read(cx).editor.read(cx).text(), "Second note");
    });
    // Each document retains its own close guard; cancelling never closes siblings.
    cx.update(|_, cx| {
        let (handle, opened) = cx.global::<Session>().windows[0].clone();
        handle
            .update(cx, |_, window, cx| {
                opened
                    .update(cx, |this, cx| {
                        this.editor.update(cx, |editor, cx| {
                            editor.command(EditorCommand::Wrap("**"), cx);
                        });
                        this.request(Intent::Close, window, cx);
                    })
                    .unwrap();
            })
            .unwrap();
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("取消");
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| cx.windows().len()), 3);
    cx.update(|_, cx| {
        let (handle, opened) = cx.global::<Session>().windows[0].clone();
        handle
            .update(cx, |_, window, cx| {
                opened
                    .update(cx, |this, cx| this.request(Intent::Close, window, cx))
                    .unwrap();
            })
            .unwrap();
    });
    cx.simulate_prompt_answer("放弃修改");
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(cx.windows().len(), 2);
        assert_eq!(view.read(cx).editor.read(cx).text(), "Current note");
        let (_, sibling) = &cx.global::<Session>().windows[1];
        let sibling = sibling.upgrade().unwrap();
        assert_eq!(sibling.read(cx).document.path(), Some(second.as_path()));
        assert_eq!(sibling.read(cx).editor.read(cx).text(), "Second note");
    });
}

#[gpui::test]
fn reload_stays_in_the_current_window_and_guards_unsaved_changes(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let first = note(directory.path(), "first.md", "First note");
    let (view, cx) = cx.add_window_view(|window, cx| TinyMd::new(Some(first.clone()), window, cx));
    std::fs::write(&first, "Updated on disk").unwrap();
    cx.dispatch_action(ReloadDocument);
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(cx.windows().len(), 1);
        assert_eq!(view.read(cx).document.path(), Some(first.as_path()));
        assert_eq!(view.read(cx).editor.read(cx).text(), "Updated on disk");
    });
    cx.update(|_, cx| {
        view.update(cx, |this, cx| {
            this.editor.update(cx, |editor, cx| {
                editor.command(EditorCommand::Wrap("**"), cx);
            });
        });
    });
    cx.dispatch_action(ReloadDocument);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("取消");
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(cx.windows().len(), 1);
        assert_eq!(view.read(cx).editor.read(cx).text(), "**Updated** on disk");
        assert!(view.read(cx).dirty);
    });
}

#[gpui::test]
fn sidebar_click_opens_a_window_and_keeps_the_library_root(cx: &mut TestAppContext) {
    init(cx);
    let directory = tempfile::tempdir().unwrap();
    let first = note(directory.path(), "first.md", "First note");
    let folder = directory.path().join("nested");
    std::fs::create_dir(&folder).unwrap();
    let second = note(&folder, "second.md", "Second note");
    let root = first.parent().unwrap().to_owned();
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(Some(first.clone()), window, cx);
        view.sidebar = true;
        view
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
    let row = cx
        .debug_bounds("document-entry-1")
        .expect("second note row");
    cx.simulate_click(row.center(), Modifiers::none());
    cx.run_until_parked();
    cx.read(|cx| {
        assert_opened(cx, &second, "Second note");
        assert_eq!(view.read(cx).document.path(), Some(first.as_path()));
        let (_, opened) = cx.global::<Session>().windows.last().unwrap();
        let opened = opened.upgrade().unwrap();
        assert_eq!(
            opened.read(cx).library_root.as_deref(),
            Some(root.as_path())
        );
    });
}

#[gpui::test]
fn an_empty_file_selection_leaves_the_current_document_unchanged(cx: &mut TestAppContext) {
    init(cx);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = TinyMd::new(None, window, cx);
        view.install(
            Document::untitled("Current note"),
            "Current note",
            window,
            cx,
        );
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |this, cx| this.open_paths(vec![], window, cx));
    });
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(cx.windows().len(), 1);
        assert_eq!(view.read(cx).editor.read(cx).text(), "Current note");
        assert!(!view.read(cx).busy);
    });
}
