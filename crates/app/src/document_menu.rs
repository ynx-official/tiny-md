use crate::*;
use gpui_component::{
    WindowExt,
    dialog::DialogButtonProps,
    menu::{PopupMenu, PopupMenuItem},
};
use std::fs;

#[derive(Clone, Copy)]
pub(crate) enum DocumentCommand {
    Open,
    OpenWindow,
    New,
    Search,
    List,
    Tree,
    Info,
    Rename,
    Duplicate,
    Delete,
    CopyPath,
    Reveal,
}

enum FileChange {
    Renamed(PathBuf),
    Duplicated,
    Deleted,
}

impl TinyMd {
    pub(crate) fn document_context_menu(
        &self,
        menu: PopupMenu,
        target: Option<PathBuf>,
        directory: bool,
        view: WeakEntity<Self>,
    ) -> PopupMenu {
        let saved = target.is_some() && !directory;
        let item = |label, command, disabled, checked| {
            let target = target.clone();
            let view = view.clone();
            PopupMenuItem::new(label)
                .disabled(disabled)
                .checked(checked)
                .on_click(move |_, window, cx| {
                    let target = target.clone();
                    let view = view.clone();
                    // Run after the popup dismisses, so dialog/input focus is retained.
                    window.defer(cx, move |window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.document_command(command, target, directory, window, cx)
                        });
                    });
                })
        };
        menu.min_w(px(215.0))
            .item(item(
                "打开",
                DocumentCommand::Open,
                self.busy || target.is_none(),
                false,
            ))
            .item(item(
                "在新窗口中打开",
                DocumentCommand::OpenWindow,
                self.busy || !saved,
                false,
            ))
            .separator()
            .item(item("新建文件", DocumentCommand::New, self.busy, false))
            .separator()
            .item(item("搜索", DocumentCommand::Search, false, false))
            .separator()
            .item(item(
                "文档列表",
                DocumentCommand::List,
                false,
                self.sidebar_mode == SidebarMode::Documents,
            ))
            .item(item(
                "文档树",
                DocumentCommand::Tree,
                false,
                self.sidebar_mode == SidebarMode::Tree,
            ))
            .separator()
            .item(item(
                "显示简介…",
                DocumentCommand::Info,
                self.busy || target.is_none(),
                false,
            ))
            .item(item(
                "重命名…",
                DocumentCommand::Rename,
                self.busy || !saved,
                false,
            ))
            .item(item(
                "创建副本",
                DocumentCommand::Duplicate,
                self.busy || !saved,
                false,
            ))
            .separator()
            .item(item(
                "删除…",
                DocumentCommand::Delete,
                self.busy || !saved,
                false,
            ))
            .separator()
            .item(item(
                "复制文件路径",
                DocumentCommand::CopyPath,
                target.is_none(),
                false,
            ))
            .item(item(
                menus::reveal_label(),
                DocumentCommand::Reveal,
                target.is_none(),
                false,
            ))
    }

    fn document_command(
        &mut self,
        command: DocumentCommand,
        target: Option<PathBuf>,
        directory: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            DocumentCommand::List => self.show_sidebar(SidebarMode::Documents, cx),
            DocumentCommand::Tree => self.show_sidebar(SidebarMode::Tree, cx),
            DocumentCommand::Search => {
                if self.sidebar_mode == SidebarMode::Outline {
                    self.sidebar_mode = SidebarMode::Documents;
                }
                self.sidebar = true;
                self.library_search_open = true;
                self.library_query
                    .update(cx, |input, cx| input.focus(window, cx));
                cx.notify();
            }
            DocumentCommand::CopyPath => {
                if let Some(path) = target {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        path.to_string_lossy().into_owned(),
                    ));
                }
            }
            DocumentCommand::Reveal => {
                if let Some(path) = target {
                    cx.reveal_path(&path);
                }
            }
            _ if self.busy => {}
            DocumentCommand::Open => {
                if let Some(path) = target {
                    if directory {
                        self.toggle_folder(path, cx);
                    } else if self.document.path() != Some(path.as_path()) {
                        self.request(Intent::OpenPath(path), window, cx);
                    }
                }
            }
            DocumentCommand::OpenWindow => {
                if let Some(path) = target {
                    open_editor_window(Some(path), false, cx);
                }
            }
            DocumentCommand::New => {
                let folder = target
                    .as_deref()
                    .and_then(|path| if directory { Some(path) } else { path.parent() })
                    .map(Path::to_owned)
                    .or_else(|| self.library_root.clone());
                if let Some(folder) = folder {
                    self.filename_dialog(None, folder, window, cx);
                } else {
                    self.request(Intent::New, window, cx);
                }
            }
            DocumentCommand::Rename => {
                if let Some(path) = target
                    && let Some(folder) = path.parent()
                {
                    self.filename_dialog(Some(path.clone()), folder.to_owned(), window, cx);
                }
            }
            DocumentCommand::Duplicate => {
                if let Some(path) = target {
                    self.file_job(path, None, false, window, cx);
                }
            }
            DocumentCommand::Delete => {
                if let Some(path) = target {
                    self.confirm_delete(path, window, cx);
                }
            }
            DocumentCommand::Info => {
                if let Some(path) = target {
                    self.file_info(path, window, cx);
                }
            }
        }
    }

    pub(crate) fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.collapsed_folders.remove(&path) {
            self.collapsed_folders.insert(path);
        }
        cx.notify();
    }

    fn filename_dialog(
        &mut self,
        source: Option<PathBuf>,
        folder: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = source
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名.md".into());
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("文件名.md");
            input.set_value(name, window, cx);
            input
        });
        self.error = false;
        let view = cx.weak_entity();
        let renaming = source.is_some();
        let focus = input.clone();
        let error = cx.new(|_| String::new());
        cx.observe(&error, |_, _, cx| cx.notify()).detach();
        window.open_dialog(cx, move |dialog, _, cx| {
            let input = input.clone();
            let view = view.clone();
            let folder = folder.clone();
            let source = source.clone();
            let message = error.read(cx).clone();
            let error = error.clone();
            let confirm_input = input.clone();
            dialog
                .title(if renaming {
                    "重命名文件"
                } else {
                    "新建文件"
                })
                .confirm()
                .w(px(420.0))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(if renaming { "重命名" } else { "创建" })
                        .cancel_text("取消"),
                )
                .child(Input::new(&input))
                .when(!message.is_empty(), |dialog| {
                    dialog.child(
                        div()
                            .mt_2()
                            .text_sm()
                            .text_color(rgb(0xc45b51))
                            .child(message),
                    )
                })
                .on_ok(move |_, window, cx| {
                    let name = confirm_input.read(cx).value();
                    view.update(cx, |this, cx| {
                        if this.busy {
                            return false;
                        }
                        let path = match library::named_path(&folder, &name) {
                            Ok(path) => path,
                            Err(issue) => {
                                error.update(cx, |message, cx| {
                                    *message = issue.to_string();
                                    cx.notify();
                                });
                                return false;
                            }
                        };
                        if source.as_deref() != Some(path.as_path()) && path.exists() {
                            error.update(cx, |message, cx| {
                                *message = "同名文件已存在，请换一个名称。".into();
                                cx.notify();
                            });
                            return false;
                        }
                        if let Some(source) = &source {
                            this.file_job(source.clone(), Some(path), false, window, cx);
                        } else {
                            this.request(Intent::CreateFile(path), window, cx);
                        }
                        true
                    })
                    .unwrap_or(true)
                })
        });
        window.defer(cx, move |window, cx| {
            focus.update(cx, |input, cx| input.focus(window, cx))
        });
    }

    pub(crate) fn create_library_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_busy(true, cx);
        let task = cx.background_executor().spawn(async move {
            let path = library::create_file(&path)?;
            Document::open(&path)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                match result {
                    Ok((document, text)) => this.install(document, &text, window, cx),
                    Err(error) => this.fail(format!("新建文件失败：{error}"), cx),
                }
            });
        })
        .detach();
    }

    fn confirm_delete(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!(
                "将“{name}”移到{}？",
                if cfg!(target_os = "macos") {
                    "废纸篓"
                } else {
                    "回收站"
                }
            ),
            Some("文件可在系统文件管理器中恢复。未保存的文档需要先保存或关闭。"),
            &["取消", "移到废纸篓 / 回收站"],
            cx,
        );
        self.set_busy(true, cx);
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                if matches!(answer, Ok(1)) {
                    this.file_job(path, None, true, window, cx);
                }
            });
        })
        .detach();
    }

    fn file_job(
        &mut self,
        path: PathBuf,
        rename: Option<PathBuf>,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.finish_input(cx);
        if delete && self.document.path() == Some(path.as_path()) && self.dirty {
            self.fail("请先保存或关闭该文档的未保存修改，再删除文件。".into(), cx);
            return;
        }
        let id = cx.entity_id();
        let views = cx.global::<Session>().windows.clone();
        let mut locked = vec![];
        for (_, view) in &views {
            if view.entity_id() == id {
                continue;
            }
            let blocked = view
                .update(cx, |other, cx| {
                    if other.document.path() != Some(path.as_path()) {
                        return false;
                    }
                    other.finish_input(cx);
                    other.busy || (delete && other.dirty)
                })
                .unwrap_or(false);
            if blocked {
                self.fail(
                    "该文档在其他窗口中正在操作或有未保存修改，请先处理后再试。".into(),
                    cx,
                );
                return;
            }
        }
        self.set_busy(true, cx);
        for (_, view) in &views {
            if view.entity_id() == id {
                continue;
            }
            let affected = view
                .update(cx, |other, cx| {
                    if other.document.path() == Some(path.as_path()) {
                        other.set_busy(true, cx);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if affected {
                locked.push(view.entity_id());
            }
        }
        let source = path.clone();
        let task = cx.background_executor().spawn(async move {
            if delete {
                trash::delete(&source).map_err(|error| std::io::Error::other(error.to_string()))?;
                Ok(FileChange::Deleted)
            } else if let Some(target) = rename {
                library::rename_file(&source, &target).map(FileChange::Renamed)
            } else {
                library::duplicate_file(&source).map(|_| FileChange::Duplicated)
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                if let Ok(change) = &result {
                    this.apply_file_change(&path, change, window, cx);
                    let recent = &mut cx.global_mut::<Session>().recent;
                    match change {
                        FileChange::Renamed(new) => {
                            for item in recent {
                                if *item == path {
                                    *item = new.clone();
                                }
                            }
                        }
                        FileChange::Deleted => recent.retain(|item| *item != path),
                        FileChange::Duplicated => {}
                    }
                    this.error = false;
                    this.status = match change {
                        FileChange::Renamed(_) => "已重命名",
                        FileChange::Deleted => "已移到废纸篓 / 回收站",
                        FileChange::Duplicated => "已创建副本",
                    }
                    .into();
                } else if let Err(error) = &result {
                    this.fail(format!("文件操作失败：{error}"), cx);
                }
                this.refresh_documents(cx);
                for (handle, view) in cx.global::<Session>().windows.clone() {
                    if view.entity_id() == id {
                        continue;
                    }
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = view.update(cx, |other, cx| {
                            if locked.contains(&view.entity_id()) {
                                other.set_busy(false, cx);
                            }
                            if let Ok(change) = &result {
                                other.apply_file_change(&path, change, window, cx);
                            }
                            other.refresh_documents(cx);
                        });
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_file_change(
        &mut self,
        path: &Path,
        change: &FileChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document.path() != Some(path) {
            return;
        }
        match change {
            FileChange::Renamed(new) => self.document.retarget_after_move(path, new),
            FileChange::Deleted => self.install(Document::untitled(""), "", window, cx),
            FileChange::Duplicated => {}
        }
        cx.notify();
    }

    fn file_info(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.set_busy(true, cx);
        let task = cx.background_executor().spawn(async move {
            let metadata = fs::metadata(&path)?;
            Ok::<_, std::io::Error>(format!(
                "名称：{}\n路径：{}\n类型：{}\n大小：{} 字节\n只读：{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                path.display(),
                if metadata.is_dir() {
                    "文件夹"
                } else {
                    "Markdown 文档"
                },
                metadata.len(),
                if metadata.permissions().readonly() {
                    "是"
                } else {
                    "否"
                }
            ))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                match result {
                    Ok(info) => {
                        let answer =
                            window.prompt(PromptLevel::Info, "文件简介", Some(&info), &["好"], cx);
                        cx.spawn(async move |_, _| {
                            let _ = answer.await;
                        })
                        .detach();
                    }
                    Err(error) => this.fail(format!("读取简介失败：{error}"), cx),
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod file_open_tests {
    use super::*;
    use crate::file_open_tests::{assert_opened, init, note};
    use core::prelude::v1::test;

    #[gpui::test]
    fn document_menu_open_creates_a_window(cx: &mut TestAppContext) {
        init(cx);
        let directory = tempfile::tempdir().unwrap();
        let first = note(directory.path(), "first.md", "First note");
        let second = note(directory.path(), "second.md", "Second note");
        let (view, cx) =
            cx.add_window_view(|window, cx| TinyMd::new(Some(first.clone()), window, cx));
        cx.update(|window, cx| {
            view.update(cx, |this, cx| {
                this.document_command(
                    DocumentCommand::Open,
                    Some(second.clone()),
                    false,
                    window,
                    cx,
                );
            });
        });
        cx.run_until_parked();
        cx.read(|cx| {
            assert_opened(cx, &second, "Second note");
            assert_eq!(view.read(cx).document.path(), Some(first.as_path()));
        });
    }
}
