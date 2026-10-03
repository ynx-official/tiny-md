use gpui::{prelude::*, *};
use gpui_component::{
    Disableable, Root, Selectable, Sizable,
    button::{Button, ButtonVariants},
};
use std::path::{Path, PathBuf};
use tiny_md_document::{Document, Heading, outline};
use tiny_md_editor::{MarkdownEditor, MarkdownEditorEvent, MarkdownStyle};

actions!(
    tiny_md,
    [
        NewDocument,
        OpenDocument,
        SaveDocument,
        SaveDocumentAs,
        ToggleSource,
        ToggleTheme,
        CloseDocument
    ]
);
const WELCOME: &str = include_str!("../../../fixtures/welcome.md");

#[derive(Clone, Copy)]
enum Intent {
    New,
    Open,
    Close,
}

struct TinyMd {
    editor: Entity<MarkdownEditor>,
    document: Document,
    headings: Vec<Heading>,
    characters: usize,
    dirty: bool,
    busy: bool,
    dark: bool,
    source_mode: bool,
    status: String,
    error: bool,
    closing_approved: bool,
}

fn editor_style(dark: bool) -> MarkdownStyle {
    MarkdownStyle {
        bare: true,
        bg: Some(rgb(if dark { 0x222529 } else { 0xffffff }).into()),
        text: Some(rgb(if dark { 0xe4e6e9 } else { 0x24292f }).into()),
        caret: Some(rgb(if dark { 0x8eac98 } else { 0x3f7154 }).into()),
        accent: Some(rgb(if dark { 0x8eac98 } else { 0x3f7154 }).into()),
        ..Default::default()
    }
}

impl TinyMd {
    fn new(initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (document, text, status, error) = match initial {
            Some(path) => match Document::open(&path) {
                Ok((document, text)) => (document, text, "已打开".into(), false),
                Err(e) => (
                    Document::untitled(WELCOME),
                    WELCOME.into(),
                    format!("打开失败：{e}"),
                    true,
                ),
            },
            None => (
                Document::untitled(WELCOME),
                WELCOME.into(),
                "准备好了".into(),
                false,
            ),
        };
        let editor = cx.new(|cx| {
            MarkdownEditor::new(cx)
                .value(&text)
                .font_size(17.0)
                .placeholder("从一个想法开始……")
                .style(editor_style(false))
        });
        cx.subscribe(
            &editor,
            |this, _, event: &MarkdownEditorEvent, cx| match event {
                MarkdownEditorEvent::Change(text) => {
                    this.dirty = this.document.is_dirty(text);
                    this.characters = text.chars().filter(|c| !c.is_whitespace()).count();
                    this.headings = outline(text);
                    this.status = if this.dirty {
                        "有未保存的修改"
                    } else {
                        "与已保存版本一致"
                    }
                    .into();
                    this.error = false;
                    cx.notify();
                }
                MarkdownEditorEvent::LinkClick(target) => {
                    if target.starts_with("https://") || target.starts_with("http://") {
                        cx.open_url(target);
                    } else {
                        this.status = format!("链接：{target}");
                        cx.notify();
                    }
                }
            },
        )
        .detach();
        editor.read(cx).focus_handle().focus(window);
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.closing_approved {
                    return true;
                }
                this.request(Intent::Close, window, cx);
                false
            })
            .unwrap_or(true)
        });
        Self {
            editor,
            document,
            headings: outline(&text),
            characters: text.chars().filter(|c| !c.is_whitespace()).count(),
            dirty: false,
            busy: false,
            dark: false,
            source_mode: false,
            status,
            error,
            closing_approved: false,
        }
    }

    fn finish_input(&mut self, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.finish_composition(cx));
        // Subscriptions can run later; decisions use the current editor text.
        self.dirty = self.document.is_dirty(&self.editor.read(cx).text());
    }

    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        self.editor
            .update(cx, |editor, cx| editor.set_read_only(busy, cx));
        cx.notify();
    }

    fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        self.status = message;
        self.error = true;
        cx.notify();
    }

    fn request(&mut self, intent: Intent, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.finish_input(cx);
        if !self.dirty {
            self.execute(intent, window, cx);
            return;
        }
        self.set_busy(true, cx);
        let answer = window.prompt(
            PromptLevel::Warning,
            "保存当前文档的修改？",
            Some("继续操作前，你可以保存文档，也可以放弃修改。"),
            &["保存并继续", "放弃修改", "取消"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match answer {
                    Ok(0) => this.save(false, Some(intent), window, cx),
                    Ok(1) => this.execute(intent, window, cx),
                    _ => {}
                }
            });
        })
        .detach();
    }

    fn execute(&mut self, intent: Intent, window: &mut Window, cx: &mut Context<Self>) {
        match intent {
            Intent::New => self.install(Document::untitled(""), "", window, cx),
            Intent::Open => self.open(window, cx),
            Intent::Close => {
                self.closing_approved = true;
                window.remove_window();
                cx.quit();
            }
        }
    }

    fn install(
        &mut self,
        document: Document,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.document = document;
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        self.headings = outline(text);
        self.characters = text.chars().filter(|c| !c.is_whitespace()).count();
        self.dirty = false;
        self.error = false;
        self.status = "准备好了".into();
        self.editor.read(cx).focus_handle().focus(window);
        cx.notify();
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_busy(true, cx);
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("打开 Markdown 文档".into()),
        });
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let loaded = match paths.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => Some(executor.spawn(async move { Document::open(&path) }).await),
                    None => None,
                },
                Ok(Ok(None)) => None,
                Ok(Err(e)) => Some(Err(std::io::Error::other(e.to_string()))),
                Err(e) => Some(Err(std::io::Error::other(e.to_string()))),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match loaded {
                    Some(Ok((document, text))) => this.install(document, &text, window, cx),
                    Some(Err(e)) => this.fail(format!("打开失败：{e}"), cx),
                    None => {}
                }
            });
        })
        .detach();
    }

    fn save(
        &mut self,
        save_as: bool,
        after: Option<Intent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.finish_input(cx);
        if !save_as && self.document.path().is_some() {
            self.write(None, after, window, cx);
            return;
        }
        self.set_busy(true, cx);
        let directory = self
            .document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = self.document.title();
        let path = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let path = path.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match path {
                    Ok(Ok(Some(path))) => this.write(Some(path), after, window, cx),
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => this.fail(format!("无法选择保存位置：{e}"), cx),
                    Err(e) => this.fail(format!("无法选择保存位置：{e}"), cx),
                }
            });
        })
        .detach();
    }

    fn write(
        &mut self,
        path: Option<PathBuf>,
        after: Option<Intent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.editor.read(cx).text();
        let mut document = self.document.clone();
        self.set_busy(true, cx);
        self.status = "正在保存……".into();
        let task = cx.background_executor().spawn(async move {
            let result = match path {
                Some(path) => document.save_as(&path, &text),
                None => document.save(&text),
            };
            result.map(|_| document)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match result {
                    Ok(document) => {
                        this.document = document;
                        this.dirty = this.document.is_dirty(&this.editor.read(cx).text());
                        this.status = "已保存".into();
                        this.error = false;
                        if let Some(intent) = after {
                            this.execute(intent, window, cx);
                        }
                    }
                    Err(e) => this.fail(format!("保存失败：{e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.source_mode = !self.source_mode;
        self.editor.update(cx, |editor, cx| {
            editor.set_source_mode(self.source_mode, cx)
        });
        self.editor.read(cx).focus_handle().focus(window);
        cx.notify();
    }

    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        if self.dark {
            guise::Theme::dark().init(cx);
        } else {
            guise::Theme::light().init(cx);
        }
        gpui_component::Theme::change(
            if self.dark {
                gpui_component::ThemeMode::Dark
            } else {
                gpui_component::ThemeMode::Light
            },
            Some(window),
            cx,
        );
        self.editor.update(cx, |editor, cx| {
            editor.set_style(editor_style(self.dark), cx)
        });
        cx.refresh_windows();
    }

    fn jump(&mut self, line: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.editor.update(cx, |editor, cx| {
            editor.edit(cx, |model| model.move_to(line, 0, false))
        });
        self.editor.read(cx).focus_handle().focus(window);
    }
}

impl Render for TinyMd {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&format!(
            "{}{} — tiny-md",
            if self.dirty { "● " } else { "" },
            self.document.title()
        ));
        window.set_window_edited(self.dirty);
        let bg = rgb(if self.dark { 0x1b1d20 } else { 0xf7f7f5 });
        let surface = rgb(if self.dark { 0x222529 } else { 0xffffff });
        let ink = rgb(if self.dark { 0xe4e6e9 } else { 0x24292f });
        let muted = rgb(if self.dark { 0x989ea7 } else { 0x7a818a });
        let border = rgb(if self.dark { 0x34383f } else { 0xe6e8eb });
        let accent = rgb(if self.dark { 0x8eac98 } else { 0x3f7154 });
        let button =
            |id: &'static str, label: &'static str| Button::new(id).label(label).small().ghost();
        let toolbar =
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(58.0))
                .px_5()
                .border_b_1()
                .border_color(border)
                .bg(surface)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_color(accent)
                                .font_weight(FontWeight::BOLD)
                                .child("tiny-md"),
                        )
                        .child(div().text_color(muted).text_sm().child("让想法变得清楚")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(button("new", "新建").disabled(self.busy).on_click(
                            cx.listener(|this, _, w, cx| this.request(Intent::New, w, cx)),
                        ))
                        .child(button("open", "打开").disabled(self.busy).on_click(
                            cx.listener(|this, _, w, cx| this.request(Intent::Open, w, cx)),
                        ))
                        .child(
                            button("save", "保存").disabled(self.busy).on_click(
                                cx.listener(|this, _, w, cx| this.save(false, None, w, cx)),
                            ),
                        )
                        .child(
                            button("save-as", "另存为").disabled(self.busy).on_click(
                                cx.listener(|this, _, w, cx| this.save(true, None, w, cx)),
                            ),
                        )
                        .child(
                            button("source", "源码")
                                .selected(self.source_mode)
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, w, cx| this.toggle_source(w, cx))),
                        )
                        .child(
                            button("theme", if self.dark { "浅色" } else { "深色" })
                                .on_click(cx.listener(|this, _, w, cx| this.toggle_theme(w, cx))),
                        ),
                );
        let sidebar = div()
            .flex()
            .flex_col()
            .w(px(210.0))
            .flex_shrink_0()
            .p_5()
            .gap_4()
            .border_r_1()
            .border_color(border)
            .child(div().text_xs().text_color(muted).child("文档大纲"))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.document.title()),
            )
            .child(div().id("outline").overflow_y_scroll().flex_1().children(
                self.headings.iter().map(|heading| {
                    let line = heading.line;
                    div()
                        .id(("heading", line))
                        .py_2()
                        .pl(px((heading.level.saturating_sub(1)) as f32 * 10.0))
                        .text_sm()
                        .text_color(if heading.level == 1 { ink } else { muted })
                        .cursor_pointer()
                        .hover(|style| style.text_color(accent))
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.jump(line, window, cx)),
                        )
                        .child(if heading.text.is_empty() {
                            "无标题".to_owned()
                        } else {
                            heading.text.clone()
                        })
                }),
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("⌘S 保存 · ⌘⇧M 源码"),
            );
        let body = div().flex().flex_1().min_h_0().child(sidebar).child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .justify_center()
                .bg(surface)
                .px_6()
                .py_5()
                .child(
                    div()
                        .w_full()
                        .max_w(px(820.0))
                        .h_full()
                        .child(self.editor.clone()),
                ),
        );
        let footer = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(32.0))
            .px_5()
            .border_t_1()
            .border_color(border)
            .text_xs()
            .text_color(muted)
            .child(
                div()
                    .text_color(if self.error { rgb(0xc45b51) } else { muted })
                    .child(self.status.clone()),
            )
            .child(format!(
                "{} 字符  ·  {}  ·  Markdown",
                self.characters,
                if self.source_mode {
                    "源码"
                } else {
                    "即时排版"
                }
            ));
        div()
            .id("tiny-md")
            .key_context("TinyMd")
            .flex()
            .flex_col()
            .size_full()
            .font_family(".AppleSystemUIFont")
            .bg(bg)
            .text_color(ink)
            .on_action(cx.listener(|this, _: &NewDocument, w, cx| this.request(Intent::New, w, cx)))
            .on_action(
                cx.listener(|this, _: &OpenDocument, w, cx| this.request(Intent::Open, w, cx)),
            )
            .on_action(cx.listener(|this, _: &SaveDocument, w, cx| this.save(false, None, w, cx)))
            .on_action(cx.listener(|this, _: &SaveDocumentAs, w, cx| this.save(true, None, w, cx)))
            .on_action(cx.listener(|this, _: &ToggleSource, w, cx| this.toggle_source(w, cx)))
            .on_action(cx.listener(|this, _: &ToggleTheme, w, cx| this.toggle_theme(w, cx)))
            .on_action(
                cx.listener(|this, _: &CloseDocument, w, cx| this.request(Intent::Close, w, cx)),
            )
            .child(toolbar)
            .child(body)
            .child(footer)
    }
}

fn main() {
    let initial = std::env::args_os()
        .skip(1)
        .find(|arg| !arg.to_string_lossy().starts_with('-'))
        .map(PathBuf::from);
    Application::new().run(move |cx| {
        gpui_component::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);
        guise::Theme::light().init(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-n", NewDocument, Some("TinyMd")),
            KeyBinding::new("cmd-o", OpenDocument, Some("TinyMd")),
            KeyBinding::new("cmd-s", SaveDocument, Some("TinyMd")),
            KeyBinding::new("cmd-shift-s", SaveDocumentAs, Some("TinyMd")),
            KeyBinding::new("cmd-shift-m", ToggleSource, Some("TinyMd")),
            KeyBinding::new("cmd-shift-t", ToggleTheme, Some("TinyMd")),
            KeyBinding::new("cmd-w", CloseDocument, Some("TinyMd")),
            KeyBinding::new("cmd-q", CloseDocument, Some("TinyMd")),
        ]);
        cx.set_menus(vec![
            Menu {
                name: "Tiny MD".into(),
                items: vec![MenuItem::action("退出 Tiny MD", CloseDocument)],
            },
            Menu {
                name: "文件".into(),
                items: vec![
                    MenuItem::action("新建", NewDocument),
                    MenuItem::action("打开…", OpenDocument),
                    MenuItem::separator(),
                    MenuItem::action("保存", SaveDocument),
                    MenuItem::action("另存为…", SaveDocumentAs),
                    MenuItem::action("关闭", CloseDocument),
                ],
            },
            Menu {
                name: "编辑".into(),
                items: vec![
                    MenuItem::action("撤销", guise::actions::Undo),
                    MenuItem::action("重做", guise::actions::Redo),
                    MenuItem::separator(),
                    MenuItem::action("剪切", guise::actions::Cut),
                    MenuItem::action("复制", guise::actions::Copy),
                    MenuItem::action("粘贴", guise::actions::Paste),
                    MenuItem::action("全选", guise::actions::SelectAll),
                ],
            },
            Menu {
                name: "显示".into(),
                items: vec![
                    MenuItem::action("切换源码模式", ToggleSource),
                    MenuItem::action("切换深浅主题", ToggleTheme),
                ],
            },
        ]);
        let bounds = Bounds::centered(None, size(px(1120.0), px(780.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(780.0), px(480.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("tiny-md".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| TinyMd::new(initial, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("open tiny-md window");
        cx.activate(true);
    });
}
