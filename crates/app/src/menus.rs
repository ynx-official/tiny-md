use gpui::*;

actions!(
    tiny_md,
    [
        NewDocument,
        NewWindow,
        OpenDocument,
        SaveDocument,
        SaveDocumentAs,
        ReloadDocument,
        RevealDocument,
        CloseDocument,
        QuitApplication,
        About,
        CheckUpdates,
        ReleaseNotes,
        Hide,
        HideOthers,
        ShowAll,
        Recent1,
        Recent2,
        Recent3,
        Recent4,
        Recent5,
        ToggleSource,
        ToggleTheme,
        LightTheme,
        DarkTheme,
        ToggleSidebar,
        ShowDocuments,
        ShowOutline,
        ShowTree,
        OpenFolder,
        ToggleToolbar,
        ToggleReadOnly,
        ToggleFocus,
        ToggleTypewriter,
        ZoomIn,
        ZoomOut,
        ActualSize,
        Find,
        Replace,
        FindNext,
        FindPrevious,
        ReplaceNext,
        ReplaceAll,
        CloseSearch,
        Heading1,
        Heading2,
        Heading3,
        Heading4,
        Heading5,
        Heading6,
        Paragraph,
        PromoteHeading,
        DemoteHeading,
        InsertTable,
        TableRowAbove,
        TableRowBelow,
        TableColumnLeft,
        TableColumnRight,
        TableDeleteRow,
        TableDeleteColumn,
        TableAlignLeft,
        TableAlignCenter,
        TableAlignRight,
        InsertCodeBlock,
        Quote,
        OrderedList,
        BulletList,
        TaskList,
        ToggleTask,
        Indent,
        Outdent,
        ParagraphAbove,
        ParagraphBelow,
        HorizontalRule,
        FrontMatter,
        Bold,
        Italic,
        InlineCode,
        Strikethrough,
        Highlight,
        Link,
        InsertImage,
        ClearFormat,
        CopyPlain,
        SelectLine,
        MoveLineUp,
        MoveLineDown,
        DeleteLine,
        DeleteBlock,
        Minimize,
        ZoomWindow,
        Fullscreen,
        WordCount,
        QuickStart,
        OpenFileMenu,
        OpenEditMenu,
        OpenParagraphMenu,
        OpenFormatMenu,
        OpenViewMenu,
        OpenThemeMenu,
        OpenHelpMenu
    ]
);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuState {
    pub source: bool,
    pub dark: bool,
    pub sidebar: bool,
    pub outline: bool,
    pub tree: bool,
    pub toolbar: bool,
    pub read_only: bool,
    pub focus: bool,
    pub typewriter: bool,
}

fn checked(label: &str, enabled: bool) -> String {
    if enabled {
        format!("✓ {label}")
    } else {
        label.into()
    }
}

pub fn install(cx: &App, state: MenuState, recent: &[std::path::PathBuf]) {
    cx.set_menus(build(state, recent, cfg!(target_os = "macos")));
}

pub(crate) fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "在 Finder 中显示"
    } else if cfg!(target_os = "windows") {
        "在资源管理器中显示"
    } else {
        "在文件管理器中显示"
    }
}

fn build(state: MenuState, recent: &[std::path::PathBuf], mac: bool) -> Vec<Menu> {
    let recent_actions: Vec<Box<dyn Action>> = vec![
        Box::new(Recent1),
        Box::new(Recent2),
        Box::new(Recent3),
        Box::new(Recent4),
        Box::new(Recent5),
    ];
    let recent_items = recent
        .iter()
        .zip(recent_actions)
        .map(|(path, action)| MenuItem::Action {
            name: path.to_string_lossy().into_owned().into(),
            action,
            os_action: None,
        })
        .collect();
    let mut menus = vec![
        Menu {
            name: "Tiny MD".into(),
            items: vec![
                MenuItem::action("关于 Tiny MD", About),
                MenuItem::action("检查更新…", CheckUpdates),
                MenuItem::separator(),
                MenuItem::os_submenu("服务", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("隐藏 Tiny MD", Hide),
                MenuItem::action("隐藏其他", HideOthers),
                MenuItem::action("显示全部", ShowAll),
                MenuItem::separator(),
                MenuItem::action("退出 Tiny MD", QuitApplication),
            ],
        },
        Menu {
            name: "文件".into(),
            items: vec![
                MenuItem::action("新建", NewDocument),
                MenuItem::action("新建窗口", NewWindow),
                MenuItem::separator(),
                MenuItem::action("打开…", OpenDocument),
                MenuItem::action("打开文件夹…", OpenFolder),
                MenuItem::submenu(Menu {
                    name: "打开最近文件".into(),
                    items: recent_items,
                }),
                MenuItem::separator(),
                MenuItem::action("保存", SaveDocument),
                MenuItem::action("另存为…", SaveDocumentAs),
                MenuItem::action("从磁盘重新加载", ReloadDocument),
                MenuItem::action(
                    if mac {
                        "在 Finder 中显示"
                    } else {
                        "在资源管理器中显示"
                    },
                    RevealDocument,
                ),
                MenuItem::separator(),
                MenuItem::action("关闭", CloseDocument),
            ],
        },
        Menu {
            name: "编辑".into(),
            items: vec![
                MenuItem::os_action("撤销", guise::actions::Undo, OsAction::Undo),
                MenuItem::os_action("重做", guise::actions::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("剪切", guise::actions::Cut, OsAction::Cut),
                MenuItem::os_action("拷贝", guise::actions::Copy, OsAction::Copy),
                MenuItem::os_action("粘贴", guise::actions::Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("全选", guise::actions::SelectAll, OsAction::SelectAll),
                MenuItem::action("复制为纯文本", CopyPlain),
                MenuItem::action("选择当前行", SelectLine),
                MenuItem::separator(),
                MenuItem::action("上移该行", MoveLineUp),
                MenuItem::action("下移该行", MoveLineDown),
                MenuItem::action("删除当前行", DeleteLine),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "查找".into(),
                    items: vec![
                        MenuItem::action("查找…", Find),
                        MenuItem::action("查找下一个", FindNext),
                        MenuItem::action("查找上一个", FindPrevious),
                        MenuItem::action("替换…", Replace),
                    ],
                }),
            ],
        },
        Menu {
            name: "段落".into(),
            items: vec![
                MenuItem::action("一级标题", Heading1),
                MenuItem::action("二级标题", Heading2),
                MenuItem::action("三级标题", Heading3),
                MenuItem::action("四级标题", Heading4),
                MenuItem::action("五级标题", Heading5),
                MenuItem::action("六级标题", Heading6),
                MenuItem::separator(),
                MenuItem::action("段落", Paragraph),
                MenuItem::separator(),
                MenuItem::action("提升标题级别", PromoteHeading),
                MenuItem::action("降低标题级别", DemoteHeading),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "表格".into(),
                    items: vec![
                        MenuItem::action("插入表格", InsertTable),
                        MenuItem::separator(),
                        MenuItem::action("上方插入行", TableRowAbove),
                        MenuItem::action("下方插入行", TableRowBelow),
                        MenuItem::action("左侧插入列", TableColumnLeft),
                        MenuItem::action("右侧插入列", TableColumnRight),
                        MenuItem::separator(),
                        MenuItem::action("左对齐", TableAlignLeft),
                        MenuItem::action("居中对齐", TableAlignCenter),
                        MenuItem::action("右对齐", TableAlignRight),
                        MenuItem::separator(),
                        MenuItem::action("删除行", TableDeleteRow),
                        MenuItem::action("删除列", TableDeleteColumn),
                    ],
                }),
                MenuItem::action("代码块", InsertCodeBlock),
                MenuItem::action("引用", Quote),
                MenuItem::separator(),
                MenuItem::action("有序列表", OrderedList),
                MenuItem::action("无序列表", BulletList),
                MenuItem::action("任务列表", TaskList),
                MenuItem::action("切换任务状态", ToggleTask),
                MenuItem::submenu(Menu {
                    name: "列表缩进".into(),
                    items: vec![
                        MenuItem::action("增加缩进", Indent),
                        MenuItem::action("减少缩进", Outdent),
                    ],
                }),
                MenuItem::separator(),
                MenuItem::action("在上方插入段落", ParagraphAbove),
                MenuItem::action("在下方插入段落", ParagraphBelow),
                MenuItem::separator(),
                MenuItem::action("水平分割线", HorizontalRule),
                MenuItem::action("YAML Front Matter", FrontMatter),
            ],
        },
        Menu {
            name: "格式".into(),
            items: vec![
                MenuItem::action("加粗", Bold),
                MenuItem::action("斜体", Italic),
                MenuItem::action("代码", InlineCode),
                MenuItem::separator(),
                MenuItem::action("删除线", Strikethrough),
                MenuItem::action("高亮", Highlight),
                MenuItem::separator(),
                MenuItem::action("超链接", Link),
                MenuItem::action("插入图片链接…", InsertImage),
                MenuItem::separator(),
                MenuItem::action("清除样式", ClearFormat),
            ],
        },
        Menu {
            name: "显示".into(),
            items: vec![
                MenuItem::action(checked("源代码模式", state.source), ToggleSource),
                MenuItem::action(checked("只读模式", state.read_only), ToggleReadOnly),
                MenuItem::separator(),
                MenuItem::action(checked("专注模式", state.focus), ToggleFocus),
                MenuItem::action(checked("打字机模式", state.typewriter), ToggleTypewriter),
                MenuItem::separator(),
                MenuItem::action(checked("工具栏", state.toolbar), ToggleToolbar),
                MenuItem::action(checked("显示 / 隐藏侧边栏", state.sidebar), ToggleSidebar),
                MenuItem::action(
                    checked("文档列表", state.sidebar && !state.outline && !state.tree),
                    ShowDocuments,
                ),
                MenuItem::action(checked("大纲", state.sidebar && state.outline), ShowOutline),
                MenuItem::action(checked("文档树", state.sidebar && state.tree), ShowTree),
                MenuItem::action("字数统计", WordCount),
                MenuItem::separator(),
                MenuItem::action("实际大小", ActualSize),
                MenuItem::action("放大", ZoomIn),
                MenuItem::action("缩小", ZoomOut),
                MenuItem::separator(),
                MenuItem::action("全屏", Fullscreen),
            ],
        },
        Menu {
            name: "主题".into(),
            items: vec![
                MenuItem::action(checked("浅色", !state.dark), LightTheme),
                MenuItem::action(checked("深色", state.dark), DarkTheme),
            ],
        },
        Menu {
            name: "窗口".into(),
            items: vec![
                MenuItem::action("最小化", Minimize),
                MenuItem::action("缩放", ZoomWindow),
                MenuItem::action("全屏", Fullscreen),
            ],
        },
        Menu {
            name: "帮助".into(),
            items: vec![
                MenuItem::action("快速入门", QuickStart),
                MenuItem::action("检查更新…", CheckUpdates),
                MenuItem::action("版本变更日志", ReleaseNotes),
                MenuItem::action("关于 Tiny MD", About),
            ],
        },
    ];
    if !mac {
        // Windows has no global application menu or macOS Services/Hide actions.
        menus.remove(0);
        menus[0].items.extend([
            MenuItem::separator(),
            MenuItem::action("退出 Tiny MD", QuitApplication),
        ]);
        // Windows exposes window controls in its titlebar. Keep the compact
        // seven-menu layout requested for the writing window.
        menus.retain(|menu| menu.name != "窗口");
        if let Some(view) = menus.iter_mut().find(|menu| menu.name == "显示") {
            view.name = "视图".into();
        }
        for (menu, access) in menus.iter_mut().zip(['F', 'E', 'P', 'O', 'V', 'T', 'H']) {
            menu.name = format!("{}({access})", menu.name).into();
        }
    }
    menus
}

pub fn bind(cx: &mut App) {
    cx.bind_keys(bindings(cfg!(target_os = "macos")));
}

fn platform_binding<A: Action>(
    key: &str,
    action: A,
    context: Option<&str>,
    mac: bool,
) -> KeyBinding {
    let key = if mac {
        key.to_owned()
    } else {
        match key {
            // A literal Cmd -> Ctrl substitution would merge these with heading
            // shortcuts and leave the Windows key in other compound gestures.
            "ctrl-cmd-1" => "ctrl-alt-1".into(),
            "ctrl-cmd-2" => "ctrl-alt-2".into(),
            "ctrl-cmd-f" => "f11".into(),
            "cmd-alt-f" => "ctrl-h".into(),
            _ => key.replace("cmd-", "ctrl-"),
        }
    };
    KeyBinding::new(&key, action, context)
}

fn bindings(mac: bool) -> Vec<KeyBinding> {
    let mut bindings = vec![
        platform_binding("cmd-n", NewDocument, None, mac),
        platform_binding("cmd-shift-n", NewWindow, None, mac),
        platform_binding("cmd-o", OpenDocument, None, mac),
        platform_binding("cmd-q", QuitApplication, None, mac),
    ];
    if mac {
        bindings.extend([
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
        ]);
    }
    macro_rules! keys { ($($key:literal => $action:expr),* $(,)?) => { bindings.extend([$ (platform_binding($key, $action, Some("TinyMd"), mac)),*]); }; }
    keys![
        "cmd-n" => NewDocument, "cmd-shift-n" => NewWindow, "cmd-o" => OpenDocument,
        "cmd-s" => SaveDocument, "cmd-shift-s" => SaveDocumentAs,
        "cmd-w" => CloseDocument, "cmd-q" => QuitApplication,
        "cmd-/" => ToggleSource, "cmd-shift-m" => ToggleSource,
        "cmd-shift-t" => ToggleTheme, "cmd-shift-l" => ToggleSidebar,
        "ctrl-cmd-1" => ShowOutline, "ctrl-cmd-2" => ShowDocuments,
        "f8" => ToggleFocus, "f9" => ToggleTypewriter,
        "cmd-f" => Find, "cmd-alt-f" => Replace, "cmd-g" => FindNext, "cmd-shift-g" => FindPrevious,
        "cmd-1" => Heading1, "cmd-2" => Heading2, "cmd-3" => Heading3,
        "cmd-4" => Heading4, "cmd-5" => Heading5, "cmd-6" => Heading6, "cmd-0" => Paragraph,
        "cmd-=" => PromoteHeading, "cmd--" => DemoteHeading,
        "cmd-alt-c" => InsertCodeBlock, "cmd-alt-q" => Quote, "cmd-alt-o" => OrderedList,
        "cmd-alt-u" => BulletList, "cmd-alt-x" => TaskList, "cmd-enter" => ToggleTask,
        "cmd-b" => Bold, "cmd-i" => Italic, "ctrl-`" => InlineCode,
        "ctrl-shift-`" => Strikethrough, "cmd-k" => Link, "cmd-backslash" => ClearFormat,
        "alt-up" => MoveLineUp, "alt-down" => MoveLineDown,
        "cmd-shift-=" => ZoomIn, "cmd-shift--" => ZoomOut, "cmd-shift-0" => ActualSize,
        "cmd-m" => Minimize, "ctrl-cmd-f" => Fullscreen,
    ];
    bindings.extend([
        platform_binding("cmd-z", guise::actions::Undo, Some("TinyMdMarkdown"), mac),
        platform_binding(
            "cmd-shift-z",
            guise::actions::Redo,
            Some("TinyMdMarkdown"),
            mac,
        ),
        platform_binding("cmd-x", guise::actions::Cut, Some("TinyMdMarkdown"), mac),
        platform_binding("cmd-c", guise::actions::Copy, Some("TinyMdMarkdown"), mac),
        platform_binding("cmd-v", guise::actions::Paste, Some("TinyMdMarkdown"), mac),
        platform_binding(
            "cmd-a",
            guise::actions::SelectAll,
            Some("TinyMdMarkdown"),
            mac,
        ),
        KeyBinding::new("escape", CloseSearch, Some("TinyMd")),
    ]);
    if !mac {
        bindings.extend([
            KeyBinding::new("alt-f", OpenFileMenu, Some("TinyMd")),
            KeyBinding::new("alt-e", OpenEditMenu, Some("TinyMd")),
            KeyBinding::new("alt-p", OpenParagraphMenu, Some("TinyMd")),
            KeyBinding::new("alt-o", OpenFormatMenu, Some("TinyMd")),
            KeyBinding::new("alt-v", OpenViewMenu, Some("TinyMd")),
            KeyBinding::new("alt-t", OpenThemeMenu, Some("TinyMd")),
            KeyBinding::new("alt-h", OpenHelpMenu, Some("TinyMd")),
            KeyBinding::new("f10", OpenFileMenu, Some("TinyMd")),
        ]);
        bindings.push(KeyBinding::new(
            "ctrl-y",
            guise::actions::Redo,
            Some("TinyMdMarkdown"),
        ));
    }
    bindings
}

pub fn register_forwarding(cx: &mut App) {
    macro_rules! forward { ($($action:ty),* $(,)?) => { $(cx.on_action(|action: &$action, cx| { crate::forward_action(action, cx); });)* }; }
    forward![
        SaveDocument,
        SaveDocumentAs,
        ReloadDocument,
        RevealDocument,
        CloseDocument,
        About,
        Recent1,
        Recent2,
        Recent3,
        Recent4,
        Recent5,
        ToggleSource,
        ToggleTheme,
        LightTheme,
        DarkTheme,
        ToggleSidebar,
        ShowDocuments,
        ShowOutline,
        ShowTree,
        OpenFolder,
        ToggleToolbar,
        ToggleReadOnly,
        ToggleFocus,
        ToggleTypewriter,
        ZoomIn,
        ZoomOut,
        ActualSize,
        Find,
        Replace,
        FindNext,
        FindPrevious,
        ReplaceNext,
        ReplaceAll,
        CloseSearch,
        Heading1,
        Heading2,
        Heading3,
        Heading4,
        Heading5,
        Heading6,
        Paragraph,
        PromoteHeading,
        DemoteHeading,
        InsertTable,
        InsertCodeBlock,
        Quote,
        OrderedList,
        BulletList,
        TaskList,
        ToggleTask,
        Indent,
        Outdent,
        ParagraphAbove,
        ParagraphBelow,
        HorizontalRule,
        FrontMatter,
        Bold,
        Italic,
        InlineCode,
        Strikethrough,
        Highlight,
        Link,
        InsertImage,
        ClearFormat,
        CopyPlain,
        SelectLine,
        MoveLineUp,
        MoveLineDown,
        DeleteLine,
        DeleteBlock,
        Minimize,
        ZoomWindow,
        Fullscreen,
        WordCount,
        QuickStart,
        TableRowAbove,
        TableRowBelow,
        TableColumnLeft,
        TableColumnRight,
        TableDeleteRow,
        TableDeleteColumn,
        TableAlignLeft,
        TableAlignCenter,
        TableAlignRight,
        guise::actions::Copy,
        guise::actions::Cut,
        guise::actions::Paste,
        guise::actions::SelectAll,
        guise::actions::Undo,
        guise::actions::Redo,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn windows_menu_access_keys_match_visible_hints_and_do_not_affect_mac_option_keys() {
        let menus = build(MenuState::default(), &[], false);
        assert_eq!(
            menus
                .iter()
                .map(|menu| menu.name.as_ref())
                .collect::<Vec<_>>(),
            [
                "文件(F)",
                "编辑(E)",
                "段落(P)",
                "格式(O)",
                "视图(V)",
                "主题(T)",
                "帮助(H)"
            ]
        );
        for (key, action) in [
            ("alt-f", Box::new(OpenFileMenu) as Box<dyn Action>),
            ("alt-e", Box::new(OpenEditMenu)),
            ("alt-p", Box::new(OpenParagraphMenu)),
            ("alt-o", Box::new(OpenFormatMenu)),
            ("alt-v", Box::new(OpenViewMenu)),
            ("alt-t", Box::new(OpenThemeMenu)),
            ("alt-h", Box::new(OpenHelpMenu)),
        ] {
            assert!(
                has_binding(&bindings(false), key, action.as_ref()),
                "missing {key}"
            );
            assert!(
                !has_binding(&bindings(true), key, action.as_ref()),
                "reserved mac Option chord {key}"
            );
        }
    }

    fn has_binding(bindings: &[KeyBinding], chord: &str, action: &dyn Action) -> bool {
        let stroke = Keystroke::parse(chord).unwrap();
        bindings.iter().any(|binding| {
            binding.action().partial_eq(action)
                && binding.match_keystrokes(std::slice::from_ref(&stroke)) == Some(false)
        })
    }

    #[test]
    fn windows_shortcuts_use_control_and_keep_sidebar_and_heading_actions_distinct() {
        let keys = bindings(false);
        for (chord, action) in [
            ("ctrl-s", Box::new(SaveDocument) as Box<dyn Action>),
            ("ctrl-shift-s", Box::new(SaveDocumentAs)),
            ("ctrl-n", Box::new(NewDocument)),
            ("ctrl-shift-n", Box::new(NewWindow)),
            ("ctrl-o", Box::new(OpenDocument)),
            ("ctrl-w", Box::new(CloseDocument)),
            ("ctrl-h", Box::new(Replace)),
            ("ctrl-alt-1", Box::new(ShowOutline)),
            ("ctrl-1", Box::new(Heading1)),
            ("f11", Box::new(Fullscreen)),
            ("ctrl-y", Box::new(guise::actions::Redo)),
            ("ctrl-shift-z", Box::new(guise::actions::Redo)),
            ("ctrl-c", Box::new(guise::actions::Copy)),
        ] {
            assert!(
                has_binding(&keys, chord, action.as_ref()),
                "missing {chord}"
            );
        }
        assert!(!keys.iter().any(|binding| has_binding(
            std::slice::from_ref(binding),
            "ctrl-h",
            &Hide
        )));
    }

    #[test]
    fn mac_shortcuts_keep_command_and_native_hide_actions() {
        let keys = bindings(true);
        assert!(has_binding(&keys, "cmd-s", &SaveDocument));
        assert!(has_binding(&keys, "cmd-h", &Hide));
        assert!(has_binding(&keys, "cmd-alt-f", &Replace));
        assert!(has_binding(&keys, "ctrl-cmd-1", &ShowOutline));
    }

    #[test]
    fn windows_menus_expose_quit_and_explorer_without_mac_system_items() {
        let menus = build(MenuState::default(), &[], false);
        assert!(!menus.iter().any(|menu| menu.name == "Tiny MD"));
        let file = menus.iter().find(|menu| menu.name == "文件(F)").unwrap();
        assert!(file.items.iter().any(|item| matches!(item,
            MenuItem::Action { action, .. } if action.partial_eq(&QuitApplication))));
        assert!(file.items.iter().any(|item| matches!(item,
            MenuItem::Action { name, .. } if name == "在资源管理器中显示")));
        let mac_menus = build(MenuState::default(), &[], true);
        assert_eq!(mac_menus[0].name, "Tiny MD");
    }

    #[test]
    fn both_platforms_expose_update_and_release_note_actions() {
        for mac in [true, false] {
            let menus = build(MenuState::default(), &[], mac);
            for action in [&CheckUpdates as &dyn Action, &ReleaseNotes] {
                assert!(
                    menus
                        .iter()
                        .flat_map(|menu| &menu.items)
                        .any(|item| matches!(item,
                    MenuItem::Action {action: item_action, ..} if item_action.partial_eq(action)))
                );
            }
        }
    }
}
