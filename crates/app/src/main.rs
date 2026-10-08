#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::{prelude::*, *};
use gpui_component::{
    Disableable, Root, Selectable, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
};
use std::path::{Path, PathBuf};
use tiny_md_document::{Document, Heading, TextAnalysis};
use tiny_md_editor::{
    BlockStyle, EditorCommand, MarkdownEditor, MarkdownEditorEvent, MarkdownStyle, TableAlignment,
    TableCommand,
};
#[cfg(target_os = "macos")]
mod app_icon;
mod assets;
mod diagram_viewer;
mod disk_sync;
mod document_menu;
mod library;
mod menus;
mod panels;
#[cfg(target_os = "windows")]
mod windows_menu;
use menus::*;
use panels::SidebarMode;
#[cfg(target_os = "windows")]
use windows_menu::WindowsMenuBar;

const WELCOME: &str = include_str!("../../../fixtures/welcome.md");

#[derive(Clone)]
enum Intent {
    New,
    Open,
    OpenPath(PathBuf),
    CreateFile(PathBuf),
    Close,
    Quit,
}

#[derive(Default)]
struct Session {
    windows: Vec<(AnyWindowHandle, WeakEntity<TinyMd>)>,
    recent: Vec<PathBuf>,
    quitting: bool,
    dark: bool,
    #[cfg(not(target_os = "windows"))]
    menu_state: Option<(WindowId, MenuState, Vec<PathBuf>)>,
    current: Option<WindowId>,
}
impl Global for Session {}

fn remember(path: &Path, cx: &mut App) {
    let session = cx.global_mut::<Session>();
    session.recent.retain(|old| old != path);
    session.recent.insert(0, path.to_owned());
    session.recent.truncate(5);
}

fn continue_quit(cx: &mut App) {
    if !cx.global::<Session>().quitting {
        return;
    }
    let windows = cx.windows();
    cx.global_mut::<Session>()
        .windows
        .retain(|(handle, entity)| windows.contains(handle) && entity.upgrade().is_some());
    let next = cx.global::<Session>().windows.first().cloned();
    if let Some((handle, entity)) = next {
        let _ = handle.update(cx, |_, window, cx| {
            let _ = entity.update(cx, |this, cx| this.request(Intent::Quit, window, cx));
        });
    } else {
        cx.quit();
    }
}

fn quit(cx: &mut App) {
    cx.global_mut::<Session>().quitting = true;
    cx.defer(continue_quit);
}

// Native menu tracking may temporarily clear NSApplication.keyWindow. Keep the
// last interacted document as the menu target without changing its text focus.
fn forward_action(action: &dyn Action, cx: &mut App) -> bool {
    let windows = cx.windows();
    let target = cx
        .active_window()
        .or_else(|| {
            windows
                .iter()
                .find(|handle| Some(handle.window_id()) == cx.global::<Session>().current)
                .copied()
        })
        .or_else(|| {
            cx.global::<Session>()
                .windows
                .iter()
                .rev()
                .find(|(handle, _)| windows.contains(handle))
                .map(|(handle, _)| *handle)
        });
    target
        .and_then(|handle| {
            handle
                .update(cx, |_, window, cx| {
                    if !window.is_action_available(action, cx) {
                        return false;
                    }
                    window.dispatch_action(action.boxed_clone(), cx);
                    true
                })
                .ok()
        })
        .unwrap_or(false)
}

struct TinyMd {
    #[cfg(target_os = "windows")]
    menu_bar: Option<(MenuState, Vec<PathBuf>, Entity<WindowsMenuBar>)>,
    editor: Entity<MarkdownEditor>,
    document: Document,
    _disk_sync_task: Task<()>,
    external_conflict: bool,
    headings: Vec<Heading>,
    analysis: TextAnalysis,
    characters: usize,
    dirty: bool,
    busy: bool,
    dark: bool,
    source_mode: bool,
    sidebar: bool,
    sidebar_mode: SidebarMode,
    library_root: Option<PathBuf>,
    documents: Vec<library::Entry>,
    library_error: Option<String>,
    library_request: u64,
    library_query: Entity<InputState>,
    library_search_open: bool,
    collapsed_folders: std::collections::BTreeSet<PathBuf>,
    toolbar: bool,
    read_only: bool,
    focus_mode: bool,
    typewriter: bool,
    font_size: f32,
    search_open: bool,
    replace_open: bool,
    query: Entity<InputState>,
    replacement: Entity<InputState>,
    status: String,
    error: bool,
    closing_approved: bool,
}

fn ui_font() -> &'static str {
    if cfg!(target_os = "macos") {
        ".AppleSystemUIFont"
    } else if cfg!(target_os = "windows") {
        "Segoe UI"
    } else {
        ".SystemUIFont"
    }
}

fn guise_theme(dark: bool) -> guise::Theme {
    let mut theme = if dark {
        guise::Theme::dark()
    } else {
        guise::Theme::light()
    };
    if cfg!(target_os = "windows") {
        theme.font_family = ui_font().into();
    }
    theme
}

#[cfg(target_os = "windows")]
fn apply_windows_theme(dark: bool, cx: &mut App) {
    let theme = gpui_component::Theme::global_mut(cx);
    theme.font_family = ui_font().into();
    // The local Notion reference supplies light workspace tokens. Keep the
    // existing dark palette, which has already been designed for this editor.
    if !dark {
        theme.background = rgb(0xffffff).into();
        theme.foreground = rgb(0x37352f).into();
        theme.border = rgb(0xe5e3df).into();
        theme.title_bar = rgb(0xf6f5f4).into();
        theme.title_bar_border = rgb(0xe5e3df).into();
        theme.popover = rgb(0xffffff).into();
        theme.popover_foreground = rgb(0x37352f).into();
        theme.secondary = rgb(0xf6f5f4).into();
        theme.secondary_foreground = rgb(0x37352f).into();
        theme.secondary_hover = rgb(0xede9e4).into();
        theme.accent = rgb(0xf0eeec).into();
        theme.accent_foreground = rgb(0x37352f).into();
        theme.primary = rgb(0x5645d4).into();
        theme.primary_active = rgb(0x4534b3).into();
        theme.primary_hover = rgb(0x4534b3).into();
        theme.ring = rgb(0x5645d4).into();
    }
}

fn editor_style(dark: bool) -> MarkdownStyle {
    MarkdownStyle {
        bare: true,
        compact_headings: cfg!(target_os = "windows"),
        bg: Some(rgb(if dark { 0x222529 } else { 0xffffff }).into()),
        text: Some(rgb(if dark { 0xe4e6e9 } else { 0x24292f }).into()),
        caret: Some(rgb(if dark { 0xe4e6e9 } else { 0x24292f }).into()),
        quote_text: Some(rgb(if dark { 0xb2b7bf } else { 0x787671 }).into()),
        quote_bar: Some(rgb(if dark { 0x737b86 } else { 0xdde0e4 }).into()),
        accent: Some(rgb(if dark { 0x8eac98 } else { 0x3f7154 }).into()),
        ..Default::default()
    }
}

impl TinyMd {
    fn update_analysis(&mut self, text: &str) {
        let work = self.analysis.update(text);
        self.characters = self.analysis.characters();
        if work.outlined {
            self.headings = self.analysis.headings().to_vec();
        }
    }

    fn new(initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dark = cx.global::<Session>().dark;
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
        if let Some(path) = document.path() {
            remember(path, cx);
        }
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("查找（区分大小写）"));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("替换为"));
        let library_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("搜索文档名称或内容"));
        cx.subscribe(&library_query, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        cx.subscribe_in(&query, window, |this, _, event: &InputEvent, window, cx| {
            match event {
                InputEvent::Change => this.find(false, false, window, cx),
                InputEvent::PressEnter { secondary } => this.find(*secondary, true, window, cx),
                _ => {}
            }
            cx.notify();
        })
        .detach();
        let editor = cx.new(|cx| {
            MarkdownEditor::new(cx)
                .value(&text)
                .font_size(16.0)
                .placeholder("从一个想法开始……")
                .style(editor_style(dark))
        });
        cx.subscribe(
            &editor,
            |this, _, event: &MarkdownEditorEvent, cx| match event {
                MarkdownEditorEvent::Change(text) => {
                    this.dirty = this.document.is_dirty(text);
                    this.update_analysis(text);
                    this.status = if this.external_conflict {
                        disk_sync::CONFLICT_NOTICE
                    } else if this.dirty {
                        "有未保存的修改"
                    } else {
                        "与已保存版本一致"
                    }
                    .into();
                    this.error = this.external_conflict;
                    if let Some(entry) = this
                        .documents
                        .iter_mut()
                        .find(|entry| Some(entry.path.as_path()) == this.document.path())
                    {
                        entry.preview = library::summary(text);
                    }
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
                MarkdownEditorEvent::ViewDiagram(diagram) => {
                    if let Err(error) = diagram_viewer::open(diagram.clone(), this.dark, cx) {
                        this.status = format!("查看流程图失败：{error}");
                        this.error = true;
                        cx.notify();
                    }
                }
            },
        )
        .detach();
        editor.read(cx).focus_handle().focus(window);
        cx.observe_window_activation(window, |_, window, cx| {
            if window.is_window_active() {
                cx.global_mut::<Session>().current = Some(window.window_handle().window_id());
                cx.notify();
            }
        })
        .detach();
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
        let library_root = document
            .path()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        let mut analysis = TextAnalysis::default();
        analysis.update(&text);
        let mut this = Self {
            #[cfg(target_os = "windows")]
            menu_bar: None,
            editor,
            document,
            _disk_sync_task: Self::start_disk_sync(window, cx),
            external_conflict: false,
            headings: analysis.headings().to_vec(),
            characters: analysis.characters(),
            analysis,
            dirty: false,
            busy: false,
            dark,
            source_mode: false,
            sidebar: false,
            sidebar_mode: SidebarMode::Documents,
            library_root,
            documents: vec![],
            library_error: None,
            library_request: 0,
            library_query,
            library_search_open: false,
            collapsed_folders: Default::default(),
            toolbar: false,
            read_only: false,
            focus_mode: false,
            typewriter: false,
            font_size: 16.0,
            search_open: false,
            replace_open: false,
            query,
            replacement,
            status,
            error,
            closing_approved: false,
        };
        this.refresh_documents(cx);
        this
    }

    fn finish_input(&mut self, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.finish_composition(cx));
        // Subscriptions can run later; decisions use the current editor text.
        self.dirty = self.document.is_dirty(&self.editor.read(cx).text());
    }

    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        self.editor.update(cx, |editor, cx| {
            editor.set_read_only(busy || self.read_only, cx)
        });
        cx.notify();
    }

    fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        cx.global_mut::<Session>().quitting = false;
        self.status = message;
        self.error = true;
        cx.notify();
    }

    fn request(&mut self, intent: Intent, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            if matches!(intent, Intent::Quit) {
                cx.global_mut::<Session>().quitting = false;
            }
            return;
        }
        self.finish_input(cx);
        if !self.dirty {
            self.execute(intent, window, cx);
            return;
        }
        let quitting = matches!(intent, Intent::Quit);
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
                    _ => {
                        if quitting {
                            cx.global_mut::<Session>().quitting = false;
                        }
                    }
                }
            });
        })
        .detach();
    }

    fn execute(&mut self, intent: Intent, window: &mut Window, cx: &mut Context<Self>) {
        match intent {
            Intent::New => self.install(Document::untitled(""), "", window, cx),
            Intent::Open => self.open(window, cx),
            Intent::OpenPath(path) => self.load_path(path, window, cx),
            Intent::CreateFile(path) => self.create_library_file(path, window, cx),
            Intent::Close | Intent::Quit => {
                self.closing_approved = true;
                window.remove_window();
                if matches!(intent, Intent::Quit) {
                    cx.defer(continue_quit);
                }
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
        self.external_conflict = false;
        if let Some(parent) = self.document.path().and_then(Path::parent)
            && !self
                .library_root
                .as_ref()
                .is_some_and(|root| parent.starts_with(root))
        {
            self.library_root = Some(parent.to_owned());
        }
        if let Some(path) = self.document.path() {
            remember(path, cx);
        }
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        self.update_analysis(text);
        self.dirty = false;
        self.error = false;
        self.status = "准备好了".into();
        self.editor.read(cx).focus_handle().focus(window);
        cx.notify();
        self.refresh_documents(cx);
    }

    fn load_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.set_busy(true, cx);
        let task = cx
            .background_executor()
            .spawn(async move { Document::open(&path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                match result {
                    Ok((document, text)) => this.install(document, &text, window, cx),
                    Err(error) => this.fail(format!("打开失败：{error}"), cx),
                }
            });
        })
        .detach();
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
        let suggested = if self.external_conflict {
            format!(
                "{}-本地副本.md",
                Path::new(&self.document.title())
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
            )
        } else {
            self.document.title()
        };
        let path = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let path = path.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match path {
                    Ok(Ok(Some(path))) => this.write(Some(path), after, window, cx),
                    Ok(Ok(None)) => {
                        cx.global_mut::<Session>().quitting = false;
                    }
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
                Some(path) => document.save_as_synchronized(&path, &text),
                None => document.save_synchronized(&text),
            };
            result.map(|saved_text| (document, saved_text))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match result {
                    Ok((document, text)) => {
                        this.adopt_synced(document, &text, "已保存", cx);
                        if let Some(parent) = this.document.path().and_then(Path::parent)
                            && !this
                                .library_root
                                .as_ref()
                                .is_some_and(|root| parent.starts_with(root))
                        {
                            this.library_root = Some(parent.to_owned());
                        }
                        this.refresh_documents(cx);
                        if let Some(path) = this.document.path() {
                            remember(path, cx);
                        }
                        this.dirty = this.document.is_dirty(&this.editor.read(cx).text());
                        this.status = "已保存".into();
                        this.error = false;
                        if let Some(intent) = after {
                            this.execute(intent, window, cx);
                        }
                    }
                    Err(e) => {
                        cx.global_mut::<Session>().quitting = false;
                        if matches!(e, tiny_md_document::SyncError::Conflict) {
                            this.mark_sync_conflict(cx);
                            this.resolve_external_conflict(after, window, cx);
                        } else {
                            this.fail(format!("保存失败：{e}"), cx);
                        }
                    }
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
        cx.global_mut::<Session>().dark = self.dark;
        guise_theme(self.dark).init(cx);
        gpui_component::Theme::change(
            if self.dark {
                gpui_component::ThemeMode::Dark
            } else {
                gpui_component::ThemeMode::Light
            },
            Some(window),
            cx,
        );
        #[cfg(target_os = "windows")]
        apply_windows_theme(self.dark, cx);
        self.editor.update(cx, |editor, cx| {
            editor.set_style(editor_style(self.dark), cx)
        });
        let dark = self.dark;
        let entity = cx.entity_id();
        let views = cx
            .global::<Session>()
            .windows
            .iter()
            .map(|(_, view)| view.clone())
            .collect::<Vec<_>>();
        cx.defer(move |cx| {
            for view in views {
                if view.entity_id() == entity {
                    continue;
                }
                let _ = view.update(cx, |this, cx| {
                    this.dark = dark;
                    this.editor
                        .update(cx, |editor, cx| editor.set_style(editor_style(dark), cx));
                    cx.notify();
                });
            }
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

    fn command(&mut self, command: EditorCommand, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.editor
            .update(cx, |editor, cx| editor.command(command, cx));
        self.editor.read(cx).focus_handle().focus(window);
    }

    fn route_edit(
        &mut self,
        document: &dyn Action,
        input: &dyn Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.query.read(cx).focus_handle(cx).is_focused(window) {
            self.query
                .read(cx)
                .focus_handle(cx)
                .dispatch_action(input, window, cx);
        } else if self
            .replacement
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
        {
            self.replacement
                .read(cx)
                .focus_handle(cx)
                .dispatch_action(input, window, cx);
        } else {
            self.editor
                .read(cx)
                .focus_handle()
                .dispatch_action(document, window, cx);
        }
    }

    fn open_search(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_input(cx);
        self.search_open = true;
        self.replace_open = replace;
        if let Some(text) = self.editor.read(cx).model().selected_text()
            && !text.contains('\n')
            && text.len() < 512
        {
            self.query
                .update(cx, |query, cx| query.set_value(text, window, cx));
        }
        self.query.update(cx, |query, cx| query.focus(window, cx));
        cx.notify();
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = false;
        self.editor.read(cx).focus_handle().focus(window);
        cx.notify();
    }

    fn find(
        &mut self,
        backwards: bool,
        advance: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let query = self.query.read(cx).value().to_string();
        self.editor
            .update(cx, |editor, cx| editor.find(&query, backwards, advance, cx));
        cx.notify();
    }

    fn replace(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.read_only {
            return;
        }
        let query = self.query.read(cx).value().to_string();
        let value = self.replacement.read(cx).value().to_string();
        self.editor.update(cx, |editor, cx| {
            if all {
                editor.replace_all(&query, &value, cx);
            } else {
                editor.replace_match(&query, &value, cx);
                editor.find(&query, false, false, cx);
            }
        });
        self.editor.read(cx).focus_handle().focus(window);
        cx.notify();
    }

    fn writing_modes(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_writing_modes(self.focus_mode, self.typewriter, cx)
        });
        cx.notify();
    }

    fn zoom(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.font_size = if delta == 0.0 {
            16.0
        } else {
            (self.font_size + delta).clamp(12.0, 30.0)
        };
        self.editor
            .update(cx, |editor, cx| editor.set_font_size(self.font_size, cx));
        cx.notify();
    }

    fn recent(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = cx.global::<Session>().recent.get(index).cloned() {
            self.request(Intent::OpenPath(path), window, cx);
        }
    }

    fn stats(&self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text();
        let message = format!(
            "{} 个非空白字符\n{} 行\n{} 字节",
            self.characters,
            text.lines().count(),
            text.len()
        );
        let answer = window.prompt(PromptLevel::Info, "文档统计", Some(&message), &["好"], cx);
        cx.spawn(async move |_, _| {
            let _ = answer.await;
        })
        .detach();
    }
}

fn about(window: &mut Window, cx: &mut App) {
    let answer = window.prompt(
        PromptLevel::Info,
        "Tiny MD",
        Some("原生 Markdown 写作应用 · 0.1.0\nRust · GPUI · GPUI Component · Guise"),
        &["好"],
        cx,
    );
    cx.spawn(async move |_| {
        let _ = answer.await;
    })
    .detach();
}

impl Render for TinyMd {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_title = if cfg!(target_os = "windows") {
            format!(
                "{}{} — Tiny MD",
                self.document.title(),
                if self.dirty { " •" } else { "" }
            )
        } else {
            self.document.title()
        };
        window.set_window_title(&window_title);
        window.set_window_edited(self.dirty);
        let menu_state = MenuState {
            source: self.source_mode,
            dark: self.dark,
            sidebar: self.sidebar,
            outline: self.sidebar_mode == SidebarMode::Outline,
            tree: self.sidebar_mode == SidebarMode::Tree,
            toolbar: self.toolbar,
            read_only: self.read_only,
            focus: self.focus_mode,
            typewriter: self.typewriter,
        };
        #[cfg(not(target_os = "windows"))]
        if cx
            .active_window()
            .is_some_and(|handle| handle == window.window_handle())
            || (cx.active_window().is_none()
                && cx.global::<Session>().current == Some(window.window_handle().window_id()))
        {
            let recent = cx.global::<Session>().recent.clone();
            let stamp = (
                window.window_handle().window_id(),
                menu_state,
                recent.clone(),
            );
            if cx.global::<Session>().menu_state.as_ref() != Some(&stamp) {
                menus::install(cx, menu_state, &recent);
                cx.global_mut::<Session>().menu_state = Some(stamp);
            }
        }
        #[cfg(target_os = "windows")]
        let menu_bar = {
            let recent = cx.global::<Session>().recent.clone();
            if self
                .menu_bar
                .as_ref()
                .is_none_or(|(state, paths, _)| *state != menu_state || *paths != recent)
            {
                // The menu bar snapshots get_menus() at creation. Each window needs
                // its own snapshot, refreshed only when checks or recent files change.
                menus::install(cx, menu_state, &recent);
                self.menu_bar = Some((menu_state, recent, WindowsMenuBar::new(window, cx)));
            }
            self.menu_bar.as_ref().unwrap().2.clone()
        };
        let surface = rgb(if self.dark { 0x222529 } else { 0xffffff });
        let ink = rgb(if self.dark { 0xe4e6e9 } else { 0x333333 });
        let muted = rgb(if self.dark { 0x989ea7 } else { 0x909090 });
        let border = rgb(if self.dark { 0x34383f } else { 0xeeeeee });
        let button =
            |id: &'static str, label: &'static str| Button::new(id).label(label).small().ghost();
        let title_surface = if cfg!(target_os = "windows") && !self.dark {
            rgb(0xf0f3f9)
        } else {
            surface
        };
        let titlebar = TitleBar::new()
            .bg(title_surface)
            .border_b_0()
            .when(cfg!(target_os = "windows"), |bar| bar.h(px(28.0)))
            .when(cfg!(target_os = "macos"), |bar| bar.pr(px(80.0)))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .when(cfg!(target_os = "windows"), |title| title.pr_3().gap_2())
                    .when(!cfg!(target_os = "windows"), |title| title.justify_center())
                    .items_center()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if cfg!(target_os = "windows") && !self.dark {
                        rgb(0x5d5b54)
                    } else {
                        muted
                    })
                    .when(cfg!(target_os = "windows"), |title| {
                        title.child(img("app/window-icon.png").size(px(16.0)).flex_shrink_0())
                    })
                    .child(div().min_w_0().text_ellipsis().child(window_title)),
            );
        let sidebar = self.sidebar_panel(cx);
        let mut search = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(border);
        if self.search_open {
            let count = self
                .editor
                .read(cx)
                .match_count(&self.query.read(cx).value());
            search = search.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(280.0)).child(Input::new(&self.query).small()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("{count} 处")),
                    )
                    .child(
                        button("previous", "上一处")
                            .on_click(cx.listener(|this, _, w, cx| this.find(true, true, w, cx))),
                    )
                    .child(
                        button("next", "下一处")
                            .on_click(cx.listener(|this, _, w, cx| this.find(false, true, w, cx))),
                    )
                    .child(
                        button("replace-toggle", "替换")
                            .selected(self.replace_open)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.replace_open = !this.replace_open;
                                cx.notify();
                            })),
                    )
                    .child(
                        button("close-search", "关闭")
                            .on_click(cx.listener(|this, _, w, cx| this.close_search(w, cx))),
                    ),
            );
            if self.replace_open {
                search = search.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(280.0))
                                .child(Input::new(&self.replacement).small()),
                        )
                        .child(
                            button("replace-next", "替换")
                                .disabled(self.busy || self.read_only)
                                .on_click(cx.listener(|this, _, w, cx| this.replace(false, w, cx))),
                        )
                        .child(
                            button("replace-all", "全部替换")
                                .disabled(self.busy || self.read_only)
                                .on_click(cx.listener(|this, _, w, cx| this.replace(true, w, cx))),
                        ),
                );
            }
        }
        let body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .when(self.sidebar && !self.focus_mode, |body| body.child(sidebar))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .relative()
                    .min_w_0()
                    .justify_center()
                    .px(px(if cfg!(target_os = "windows") {
                        12.0
                    } else {
                        32.0
                    }))
                    .pt(px(if cfg!(target_os = "windows") {
                        16.0
                    } else {
                        32.0
                    }))
                    .when(self.toolbar, |body| body.pb(px(72.0)))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(if cfg!(target_os = "windows") {
                                1080.0
                            } else {
                                900.0
                            }))
                            .h_full()
                            .child(self.editor.clone()),
                    )
                    .when(self.toolbar, |body| {
                        body.child(
                            div()
                                .absolute()
                                .bottom(px(22.0))
                                .left_0()
                                .right_0()
                                .flex()
                                .justify_center()
                                .child(self.formatting_toolbar(cx)),
                        )
                    }),
            );
        let footer = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(24.0))
            .when(cfg!(target_os = "windows"), |footer| {
                footer.h(px(28.0)).border_t_1().border_color(if self.dark {
                    border
                } else {
                    rgb(0xe5e3df)
                })
            })
            .px_4()
            .text_size(px(11.0))
            .text_color(muted)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(cfg!(target_os = "windows"), |footer| {
                        footer
                            .child(
                                Button::new("footer-sidebar")
                                    .icon(gpui_component::Icon::default().path("icons/sidebar.svg"))
                                    .tooltip("显示 / 隐藏侧边栏（Ctrl+Shift+L）")
                                    .small()
                                    .ghost()
                                    .h(px(22.0))
                                    .w(px(26.0))
                                    .tab_stop(false)
                                    .selected(self.sidebar)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar = !this.sidebar;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("footer-source")
                                    .icon(gpui_component::Icon::default().path("toolbar/code.svg"))
                                    .tooltip(if self.source_mode {
                                        "退出源代码模式（Ctrl+/）"
                                    } else {
                                        "启用源代码模式（Ctrl+/）"
                                    })
                                    .small()
                                    .ghost()
                                    .h(px(22.0))
                                    .w(px(26.0))
                                    .tab_stop(false)
                                    .selected(self.source_mode)
                                    .disabled(self.busy)
                                    .on_click(
                                        cx.listener(|this, _, w, cx| this.toggle_source(w, cx)),
                                    ),
                            )
                    })
                    .text_color(if self.error { rgb(0xc45b51) } else { muted })
                    .when(self.external_conflict, |footer| {
                        footer.child(
                            Button::new("resolve-sync-conflict")
                                .label("处理同步冲突")
                                .small()
                                .ghost()
                                .h(px(22.0))
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.resolve_external_conflict(None, window, cx);
                                })),
                        )
                    })
                    .child(if self.error || self.busy {
                        self.status.clone()
                    } else {
                        String::new()
                    }),
            )
            .child(
                div()
                    .id("statistics")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, w, cx| this.stats(w, cx)))
                    .child(format!(
                        "{}{}{} 字符",
                        if self.read_only { "只读 · " } else { "" },
                        if self.source_mode { "源码 · " } else { "" },
                        self.characters
                    )),
            );
        let mut root = div()
            .id("tiny-md")
            .key_context("TinyMd")
            .flex()
            .flex_col()
            .size_full()
            .font_family(ui_font())
            .bg(surface)
            .text_color(ink)
            .capture_any_mouse_down(cx.listener(|_, _, window, cx| {
                cx.global_mut::<Session>().current = Some(window.window_handle().window_id());
                cx.notify();
            }))
            .capture_key_down(cx.listener(|_, _, window, cx| {
                cx.global_mut::<Session>().current = Some(window.window_handle().window_id());
            }))
            .on_action(cx.listener(|this, _: &NewDocument, w, cx| this.request(Intent::New, w, cx)))
            .on_action(cx.listener(|_, _: &NewWindow, _, cx| open_editor_window(None, true, cx)))
            .on_action(
                cx.listener(|this, _: &OpenDocument, w, cx| this.request(Intent::Open, w, cx)),
            )
            .on_action(cx.listener(|this, _: &SaveDocument, w, cx| this.save(false, None, w, cx)))
            .on_action(cx.listener(|this, _: &SaveDocumentAs, w, cx| this.save(true, None, w, cx)))
            .on_action(
                cx.listener(|this, _: &CloseDocument, w, cx| this.request(Intent::Close, w, cx)),
            )
            .on_action(cx.listener(|_, _: &QuitApplication, _, cx| quit(cx)))
            .on_action(cx.listener(|this, _: &ReloadDocument, w, cx| {
                if let Some(path) = this.document.path().map(Path::to_owned) {
                    this.request(Intent::OpenPath(path), w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &RevealDocument, _, cx| {
                if let Some(path) = this.document.path() {
                    cx.reveal_path(path);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSource, w, cx| this.toggle_source(w, cx)))
            .on_action(cx.listener(|this, _: &ToggleTheme, w, cx| this.toggle_theme(w, cx)))
            .on_action(cx.listener(|this, _: &LightTheme, w, cx| {
                if this.dark {
                    this.toggle_theme(w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &DarkTheme, w, cx| {
                if !this.dark {
                    this.toggle_theme(w, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar = !this.sidebar;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ShowDocuments, _, cx| {
                this.show_sidebar(SidebarMode::Documents, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowOutline, _, cx| {
                this.show_sidebar(SidebarMode::Outline, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ShowTree, _, cx| this.show_sidebar(SidebarMode::Tree, cx)),
            )
            .on_action(cx.listener(|this, _: &OpenFolder, w, cx| this.open_folder(w, cx)))
            .on_action(cx.listener(|this, _: &InsertImage, w, cx| this.insert_image(w, cx)))
            .on_action(cx.listener(|this, _: &CopyPlain, window, cx| {
                if this.query.read(cx).focus_handle(cx).is_focused(window)
                    || this
                        .replacement
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                {
                    this.route_edit(
                        &guise::actions::Copy,
                        &gpui_component::input::Copy,
                        window,
                        cx,
                    );
                } else {
                    this.editor.update(cx, |editor, cx| editor.copy_plain(cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleToolbar, _, cx| {
                this.toolbar = !this.toolbar;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleReadOnly, _, cx| {
                this.read_only = !this.read_only;
                this.set_busy(this.busy, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleFocus, _, cx| {
                this.focus_mode = !this.focus_mode;
                this.writing_modes(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleTypewriter, _, cx| {
                this.typewriter = !this.typewriter;
                this.writing_modes(cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom(1.0, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom(-1.0, cx)))
            .on_action(cx.listener(|this, _: &ActualSize, _, cx| this.zoom(0.0, cx)))
            .on_action(cx.listener(|this, _: &Find, w, cx| this.open_search(false, w, cx)))
            .on_action(cx.listener(|this, _: &Replace, w, cx| this.open_search(true, w, cx)))
            .on_action(cx.listener(|this, _: &FindNext, w, cx| this.find(false, true, w, cx)))
            .on_action(cx.listener(|this, _: &FindPrevious, w, cx| this.find(true, true, w, cx)))
            .on_action(cx.listener(|this, _: &ReplaceNext, w, cx| this.replace(false, w, cx)))
            .on_action(cx.listener(|this, _: &ReplaceAll, w, cx| this.replace(true, w, cx)))
            .on_action(cx.listener(|this, _: &CloseSearch, w, cx| this.close_search(w, cx)))
            .on_action(cx.listener(|_, _: &Minimize, w, _| w.minimize_window()))
            .on_action(cx.listener(|_, _: &ZoomWindow, w, _| w.zoom_window()))
            .on_action(cx.listener(|_, _: &Fullscreen, w, _| w.toggle_fullscreen()))
            .on_action(cx.listener(|this, _: &WordCount, w, cx| this.stats(w, cx)))
            .on_action(cx.listener(|_, _: &About, w, cx| about(w, cx)))
            .on_action(cx.listener(|_, _: &Hide, _, cx| cx.hide()))
            .on_action(cx.listener(|_, _: &HideOthers, _, cx| cx.hide_other_apps()))
            .on_action(cx.listener(|_, _: &ShowAll, _, cx| cx.unhide_other_apps()))
            .on_action(cx.listener(|_, _: &QuickStart, _, cx| open_editor_window(None, false, cx)));
        macro_rules! commands {
            ($($action:ty => $command:expr),* $(,)?) => {
                $(root = root.on_action(cx.listener(|this, _: &$action, w, cx| this.command($command, w, cx)));)*
            };
        }
        commands![
            Heading1 => EditorCommand::Block(BlockStyle::Heading(1)), Heading2 => EditorCommand::Block(BlockStyle::Heading(2)),
            Heading3 => EditorCommand::Block(BlockStyle::Heading(3)), Heading4 => EditorCommand::Block(BlockStyle::Heading(4)),
            Heading5 => EditorCommand::Block(BlockStyle::Heading(5)), Heading6 => EditorCommand::Block(BlockStyle::Heading(6)),
            Paragraph => EditorCommand::Block(BlockStyle::Paragraph), PromoteHeading => EditorCommand::HeadingDelta(-1),
            DemoteHeading => EditorCommand::HeadingDelta(1), InsertTable => EditorCommand::Table,
            InsertCodeBlock => EditorCommand::CodeBlock, Quote => EditorCommand::Block(BlockStyle::Quote),
            OrderedList => EditorCommand::Block(BlockStyle::Ordered), BulletList => EditorCommand::Block(BlockStyle::Bullet),
            TaskList => EditorCommand::Block(BlockStyle::Task), ToggleTask => EditorCommand::ToggleTask,
            Indent => EditorCommand::Indent(false), Outdent => EditorCommand::Indent(true),
            ParagraphAbove => EditorCommand::ParagraphAbove, ParagraphBelow => EditorCommand::ParagraphBelow,
            HorizontalRule => EditorCommand::Rule, FrontMatter => EditorCommand::FrontMatter,
            Bold => EditorCommand::Wrap("**"), Italic => EditorCommand::Wrap("*"), InlineCode => EditorCommand::Wrap("`"),
            Strikethrough => EditorCommand::Wrap("~~"), Highlight => EditorCommand::Wrap("=="), Link => EditorCommand::Link,
            ClearFormat => EditorCommand::ClearFormat, SelectLine => EditorCommand::SelectLine,
            DeleteBlock => EditorCommand::DeleteBlock,
            MoveLineUp => EditorCommand::MoveLine(false), MoveLineDown => EditorCommand::MoveLine(true), DeleteLine => EditorCommand::DeleteLine,
            TableRowAbove => EditorCommand::TableEdit(TableCommand::RowAbove),
            TableRowBelow => EditorCommand::TableEdit(TableCommand::RowBelow),
            TableColumnLeft => EditorCommand::TableEdit(TableCommand::ColumnLeft),
            TableColumnRight => EditorCommand::TableEdit(TableCommand::ColumnRight),
            TableDeleteRow => EditorCommand::TableEdit(TableCommand::DeleteRow),
            TableDeleteColumn => EditorCommand::TableEdit(TableCommand::DeleteColumn),
            TableAlignLeft => EditorCommand::TableEdit(TableCommand::Align(TableAlignment::Left)),
            TableAlignCenter => EditorCommand::TableEdit(TableCommand::Align(TableAlignment::Center)),
            TableAlignRight => EditorCommand::TableEdit(TableCommand::Align(TableAlignment::Right)),
        ];
        macro_rules! recent { ($($action:ty => $index:expr),*) => { $(root = root.on_action(cx.listener(|this, _: &$action, w, cx| this.recent($index, w, cx)));)* }; }
        recent![Recent1 => 0, Recent2 => 1, Recent3 => 2, Recent4 => 3, Recent5 => 4];
        #[cfg(target_os = "windows")]
        {
            macro_rules! menu_actions { ($($action:ty => $index:expr),* $(,)?) => {
                $(root = root.on_action(cx.listener(|this, _: &$action, window, cx| {
                    if let Some((_, _, menu)) = &this.menu_bar {
                        menu.update(cx, |menu, cx| menu.open($index, window, cx));
                    }
                }));)*
            }; }
            menu_actions![OpenFileMenu => 0, OpenEditMenu => 1, OpenParagraphMenu => 2,
                OpenFormatMenu => 3, OpenViewMenu => 4, OpenThemeMenu => 5, OpenHelpMenu => 6];
        }
        macro_rules! edit_actions { ($($document:ty => $input:expr),*) => {
            $(root = root.on_action(cx.listener(|this, action: &$document, w, cx| this.route_edit(action, &$input, w, cx)));)*
        }; }
        edit_actions![
            guise::actions::Copy => gpui_component::input::Copy,
            guise::actions::Cut => gpui_component::input::Cut,
            guise::actions::Paste => gpui_component::input::Paste,
            guise::actions::SelectAll => gpui_component::input::SelectAll,
            guise::actions::Undo => gpui_component::input::Undo,
            guise::actions::Redo => gpui_component::input::Redo
        ];
        let root = root.child(titlebar);
        #[cfg(target_os = "windows")]
        let root = root.child(
            div()
                .id("windows-menu-row")
                .flex()
                .items_center()
                .h(px(24.0))
                .flex_shrink_0()
                .px_2()
                .bg(surface)
                .border_b_1()
                .border_color(if self.dark { border } else { rgb(0xe5e3df) })
                .child(menu_bar),
        );
        root.when(self.search_open, |root| root.child(search))
            .child(body)
            .child(footer)
            .children(Root::render_dialog_layer(window, cx))
    }
}

fn open_editor_window(initial: Option<PathBuf>, empty: bool, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1120.0), px(780.0)), cx);
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(600.0), px(400.0))),
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| {
                let mut view = TinyMd::new(initial, window, cx);
                if empty {
                    view.install(Document::untitled(""), "", window, cx);
                }
                view
            });
            cx.global_mut::<Session>()
                .windows
                .push((window.window_handle(), view.downgrade()));
            cx.global_mut::<Session>().current = Some(window.window_handle().window_id());
            cx.new(|cx| Root::new(view, window, cx))
        },
    );
    if result.is_ok() {
        cx.activate(true);
    }
}

fn main() {
    let initial = std::env::args_os()
        .skip(1)
        .find(|arg| !arg.to_string_lossy().starts_with('-'))
        .map(PathBuf::from);
    Application::new()
        .with_assets(assets::Assets)
        .run(move |cx| {
            #[cfg(target_os = "macos")]
            app_icon::install();
            gpui_component::init(cx);
            gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx);
            #[cfg(target_os = "windows")]
            apply_windows_theme(false, cx);
            guise_theme(false).init(cx);
            cx.set_global(Session::default());
            menus::bind(cx);
            menus::install(cx, MenuState::default(), &[]);
            cx.on_action(|_: &QuitApplication, cx| quit(cx));
            cx.on_action(|action: &NewDocument, cx| {
                if !forward_action(action, cx) {
                    open_editor_window(None, true, cx);
                }
            });
            cx.on_action(|_: &NewWindow, cx| open_editor_window(None, true, cx));
            cx.on_action(|action: &OpenDocument, cx| {
                if forward_action(action, cx) {
                    return;
                }
                open_editor_window(None, true, cx);
                if let Some((handle, view)) = cx.global::<Session>().windows.last().cloned() {
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ =
                                view.update(cx, |this, cx| this.request(Intent::Open, window, cx));
                        });
                    });
                }
            });
            cx.on_action(|_: &Hide, cx| cx.hide());
            cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
            cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
            menus::register_forwarding(cx);
            let empty = cfg!(target_os = "windows") && initial.is_none();
            open_editor_window(initial, empty, cx);
        });
}
