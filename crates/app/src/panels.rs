use crate::*;
use gpui_component::{Icon, IconName, menu::DropdownMenu};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    Documents,
    Outline,
}

impl TinyMd {
    pub(crate) fn show_sidebar(&mut self, mode: SidebarMode, cx: &mut Context<Self>) {
        self.sidebar = true;
        self.sidebar_mode = mode;
        cx.notify();
    }

    pub(crate) fn refresh_documents(&mut self, cx: &mut Context<Self>) {
        self.library_request += 1;
        let request = self.library_request;
        let root = self.library_root.clone();
        let recent = cx.global::<Session>().recent.clone();
        let task = cx
            .background_executor()
            .spawn(async move { library::load(root.as_deref(), recent) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.library_request != request {
                    return;
                }
                match result {
                    Ok(mut entries) => {
                        if let Some(entry) = entries
                            .iter_mut()
                            .find(|entry| Some(entry.path.as_path()) == this.document.path())
                        {
                            entry.preview = library::summary(&this.editor.read(cx).text());
                        }
                        this.documents = entries;
                        this.library_error = None;
                    }
                    Err(error) => this.library_error = Some(format!("无法读取文件夹：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.set_busy(true, cx);
        let path = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择 Markdown 文档文件夹".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = path.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match result {
                    Ok(Ok(Some(paths))) => {
                        this.library_root = paths.into_iter().next();
                        this.show_sidebar(SidebarMode::Documents, cx);
                        this.refresh_documents(cx);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.fail(format!("打开文件夹失败：{error}"), cx),
                    Err(error) => this.fail(format!("打开文件夹失败：{error}"), cx),
                }
            });
        })
        .detach();
    }

    pub(crate) fn insert_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.read_only {
            return;
        }
        self.finish_input(cx);
        self.set_busy(true, cx);
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("插入图片链接".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            let target = path
                                .to_string_lossy()
                                .replace('<', "%3C")
                                .replace('>', "%3E")
                                .replace('\n', "%0A")
                                .replace('\r', "%0D");
                            let source = format!("![图片](<{target}>)");
                            this.editor.update(cx, |editor, cx| {
                                editor.edit(cx, |model| model.insert(&source))
                            });
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.fail(format!("插入图片失败：{error}"), cx),
                    Err(error) => this.fail(format!("插入图片失败：{error}"), cx),
                }
            });
        })
        .detach();
    }

    pub(crate) fn sidebar_panel(&self, cx: &mut Context<Self>) -> Div {
        let background = rgb(if self.dark { 0x1e2024 } else { 0xf9f9f9 });
        let ink = rgb(if self.dark { 0xe4e6e9 } else { 0x4b4b4b });
        let muted = rgb(if self.dark { 0x989ea7 } else { 0x999999 });
        let selected = rgb(if self.dark { 0x30343b } else { 0xeeeeee });
        let tabs = div()
            .flex()
            .justify_center()
            .items_center()
            .gap_2()
            .h(px(44.0))
            .children(
                [
                    (SidebarMode::Documents, "文档"),
                    (SidebarMode::Outline, "大纲"),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (mode, label))| {
                    Button::new(("sidebar-mode", i))
                        .label(label)
                        .small()
                        .ghost()
                        .tab_stop(false)
                        .selected(self.sidebar_mode == mode)
                        .on_click(cx.listener(move |this, _, _, cx| this.show_sidebar(mode, cx)))
                }),
            );
        let mut list = div()
            .id("sidebar-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        if self.sidebar_mode == SidebarMode::Outline {
            list = list.px_4().children(self.headings.iter().map(|heading| {
                let line = heading.line;
                div()
                    .id(("heading", line))
                    .py_2()
                    .pl(px(heading.level.saturating_sub(1) as f32 * 10.0))
                    .text_sm()
                    .text_color(if heading.level <= 2 { ink } else { muted })
                    .cursor_pointer()
                    .hover(|style| style.bg(selected))
                    .on_click(cx.listener(move |this, _, w, cx| this.jump(line, w, cx)))
                    .child(if heading.text.is_empty() {
                        "无标题".into()
                    } else {
                        heading.text.clone()
                    })
            }));
            if self.headings.is_empty() {
                list = list.child(
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child("文档中还没有标题"),
                );
            }
        } else {
            let mut entries = self.documents.clone();
            if let Some(path) = self.document.path()
                && !entries.iter().any(|entry| entry.path == path)
            {
                entries.insert(
                    0,
                    library::Entry {
                        path: path.to_owned(),
                        preview: library::summary(&self.editor.read(cx).text()),
                    },
                );
            }
            if self.document.path().is_none() {
                list = list.child(
                    div()
                        .px_4()
                        .py_3()
                        .bg(selected)
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(ink)
                                .child(self.document.title()),
                        )
                        .child(
                            div()
                                .mt_1()
                                .text_sm()
                                .text_color(muted)
                                .child(library::summary(&self.editor.read(cx).text())),
                        ),
                );
            }
            list = list.children(entries.into_iter().enumerate().map(|(i, entry)| {
                let active = Some(entry.path.as_path()) == self.document.path();
                let title = entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let path = entry.path;
                div()
                    .id(("document-entry", i))
                    .px_4()
                    .py_3()
                    .w_full()
                    .when(active, |row| row.bg(selected))
                    .cursor_pointer()
                    .hover(|style| style.bg(selected))
                    .on_click(cx.listener(move |this, _, w, cx| {
                        if this.document.path() != Some(path.as_path()) {
                            this.request(Intent::OpenPath(path.clone()), w, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(ink)
                                    .text_ellipsis()
                                    .child(title),
                            )
                            .when(active && self.dirty, |row| {
                                row.child(div().text_color(muted).child("•"))
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .max_h(px(54.0))
                            .overflow_hidden()
                            .text_color(muted)
                            .child(entry.preview),
                    )
            }));
            if let Some(error) = &self.library_error {
                list = list.child(
                    div()
                        .px_4()
                        .py_3()
                        .text_sm()
                        .text_color(muted)
                        .child(error.clone()),
                );
            }
        }
        div()
            .flex()
            .flex_col()
            .w(px(250.0))
            .flex_shrink_0()
            .bg(background)
            .border_r_1()
            .border_color(selected)
            .child(tabs)
            .child(list)
            .when(self.sidebar_mode == SidebarMode::Documents, |panel| {
                panel.child(
                    div()
                        .px_3()
                        .py_2()
                        .flex()
                        .justify_between()
                        .child(
                            Button::new("open-folder")
                                .label("打开文件夹…")
                                .small()
                                .ghost()
                                .tab_stop(false)
                                .on_click(cx.listener(|this, _, w, cx| this.open_folder(w, cx))),
                        )
                        .child(
                            Button::new("refresh-documents")
                                .label("刷新")
                                .small()
                                .ghost()
                                .tab_stop(false)
                                .on_click(cx.listener(|this, _, _, cx| this.refresh_documents(cx))),
                        ),
                )
            })
    }

    pub(crate) fn formatting_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let border = rgb(if self.dark { 0x42474f } else { 0xdedede });
        let surface = rgb(if self.dark { 0x2b2e33 } else { 0xffffff });
        let muted = rgb(if self.dark { 0xb6bbc3 } else { 0x858585 });
        let disabled = self.busy || self.read_only;
        let focus = self.editor.read(cx).focus_handle();
        let paragraph_focus = focus.clone();
        let list_focus = focus.clone();
        let source = self.source_mode;
        let separator = || div().w(px(1.0)).h(px(22.0)).mx_1().bg(border);
        let icon = |id: &'static str, path: &'static str, tooltip: &'static str| {
            Button::new(id)
                .icon(Icon::default().path(path))
                .tooltip(tooltip)
                .ghost()
                .small()
                .tab_stop(false)
                .disabled(disabled)
                .rounded(px(8.0))
        };
        let command = |id, path, tooltip, command: EditorCommand| {
            icon(id, path, tooltip)
                .on_click(cx.listener(move |this, _, w, cx| this.command(command, w, cx)))
        };
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .p(px(6.0))
            .rounded(px(24.0))
            .bg(surface)
            .border_1()
            .border_color(border)
            .shadow_sm()
            .text_color(muted)
            .child(
                Button::new("block-picker")
                    .label(self.editor.read(cx).block_label())
                    .dropdown_caret(true)
                    .small()
                    .ghost()
                    .tab_stop(false)
                    .disabled(disabled)
                    .dropdown_menu_with_anchor(Corner::BottomLeft, move |menu, _, _| {
                        menu.action_context(paragraph_focus.clone())
                            .min_w(px(160.0))
                            .menu("段落", Box::new(Paragraph))
                            .separator()
                            .menu("一级标题", Box::new(Heading1))
                            .menu("二级标题", Box::new(Heading2))
                            .menu("三级标题", Box::new(Heading3))
                            .menu("四级标题", Box::new(Heading4))
                            .menu("五级标题", Box::new(Heading5))
                            .menu("六级标题", Box::new(Heading6))
                            .separator()
                            .menu("代码块", Box::new(InsertCodeBlock))
                            .menu("表格", Box::new(InsertTable))
                    }),
            )
            .child(separator())
            .child(command(
                "format-bold",
                "toolbar/bold.svg",
                "加粗 ⌘B",
                EditorCommand::Wrap("**"),
            ))
            .child(command(
                "format-italic",
                "toolbar/italic.svg",
                "斜体 ⌘I",
                EditorCommand::Wrap("*"),
            ))
            .child(command(
                "format-code",
                "toolbar/code.svg",
                "行内代码",
                EditorCommand::Wrap("`"),
            ))
            .child(command(
                "format-link",
                "toolbar/link.svg",
                "链接 ⌘K",
                EditorCommand::Link,
            ))
            .child(separator())
            .child(
                icon("format-image", "toolbar/image.svg", "插入图片链接")
                    .on_click(cx.listener(|this, _, w, cx| this.insert_image(w, cx))),
            )
            .child(command(
                "format-quote",
                "toolbar/quote.svg",
                "引用",
                EditorCommand::Block(BlockStyle::Quote),
            ))
            .child(
                icon("format-list", "toolbar/list.svg", "列表")
                    .dropdown_caret(true)
                    .dropdown_menu_with_anchor(Corner::BottomRight, move |menu, _, _| {
                        menu.action_context(list_focus.clone())
                            .menu("无序列表", Box::new(BulletList))
                            .menu("有序列表", Box::new(OrderedList))
                            .menu("任务列表", Box::new(TaskList))
                            .separator()
                            .menu("增加缩进", Box::new(Indent))
                            .menu("减少缩进", Box::new(Outdent))
                    }),
            )
            .child(separator())
            .child(
                Button::new("format-more")
                    .icon(IconName::Ellipsis)
                    .tooltip("更多")
                    .small()
                    .ghost()
                    .tab_stop(false)
                    .rounded(px(10.0))
                    .dropdown_menu_with_anchor(Corner::BottomRight, move |menu, window, cx| {
                        let clipboard_focus = focus.clone();
                        menu.action_context(focus.clone())
                            .min_w(px(210.0))
                            .menu_with_check("源代码模式", source, Box::new(ToggleSource))
                            .separator()
                            .menu_with_disabled(
                                "在上方插入段落",
                                Box::new(ParagraphAbove),
                                disabled,
                            )
                            .menu_with_disabled(
                                "在下方插入段落",
                                Box::new(ParagraphBelow),
                                disabled,
                            )
                            .separator()
                            .menu("复制", Box::new(guise::actions::Copy))
                            .menu_with_disabled("粘贴", Box::new(guise::actions::Paste), disabled)
                            .submenu("复制 / 粘贴为…", window, cx, move |menu, _, _| {
                                menu.action_context(clipboard_focus.clone())
                                    .menu("复制为纯文本", Box::new(CopyPlain))
                                    .menu("复制为 Markdown", Box::new(guise::actions::Copy))
                                    .menu_with_disabled(
                                        "粘贴为纯文本",
                                        Box::new(guise::actions::Paste),
                                        disabled,
                                    )
                            })
                            .menu_with_disabled("清除样式", Box::new(ClearFormat), disabled)
                            .separator()
                            .menu_with_disabled("删除块", Box::new(DeleteBlock), disabled)
                            .separator()
                            .menu("隐藏工具栏", Box::new(ToggleToolbar))
                    }),
            )
    }
}
