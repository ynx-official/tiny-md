//! Windows access keys with GPUI Component popups and their keybinding hints.
use gpui::{prelude::*, *};
use gpui_component::{
    Selectable, Sizable,
    button::{Button, ButtonVariants},
    menu::PopupMenu,
};

pub(crate) struct WindowsMenuBar {
    menus: Vec<OwnedMenu>,
    active: Option<(usize, Entity<PopupMenu>, Subscription)>,
    origin_focus: Option<FocusHandle>,
    switch_actions: [Box<dyn Action>; 2],
}

impl WindowsMenuBar {
    pub(crate) fn new(_: &mut Window, cx: &mut App) -> Entity<Self> {
        let menus = cx.get_menus().unwrap_or_default();
        // PopupMenu propagates these actions, but the component crate keeps their
        // Rust types private. GPUI's public action registry exposes boxed actions.
        let switch_actions = ["ui::SelectLeft", "ui::SelectRight"].map(|name| {
            cx.build_action(name, None)
                .expect("GPUI Component menu action must be registered")
        });
        cx.new(|_| Self {
            menus,
            active: None,
            origin_focus: None,
            switch_actions,
        })
    }

    pub(crate) fn open(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.menus.get(index) else {
            return;
        };
        let items = menu.items.clone();
        if self.active.is_none() {
            self.origin_focus = window.focused(cx);
        }
        // Keep the editor/input focus across hover, arrow and Alt-menu switches.
        // It is both the action target and the source of shortcut hints.
        let focus = self.origin_focus.clone();
        self.active.take();
        let popup = PopupMenu::build(window, cx, move |menu, window, cx| {
            fill_popup(menu, items, focus, window, cx)
        });
        let subscription = cx.subscribe_in(&popup, window, |this, _, _: &DismissEvent, _, cx| {
            this.active.take();
            this.origin_focus.take();
            cx.notify();
        });
        popup.read(cx).focus_handle(cx).focus(window);
        self.active = Some((index, popup, subscription));
        cx.notify();
    }

    fn toggle(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .active
            .as_ref()
            .is_some_and(|(active, _, _)| *active == index)
        {
            self.active.take();
            if let Some(focus) = self.origin_focus.take() {
                focus.focus(window);
            }
            cx.notify();
        } else {
            self.open(index, window, cx);
        }
    }

    fn move_menu(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active.as_ref().map(|(index, _, _)| *index) else {
            return;
        };
        let count = self.menus.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        self.open(next, window, cx);
    }
}

fn fill_popup(
    mut popup: PopupMenu,
    items: Vec<OwnedMenuItem>,
    focus: Option<FocusHandle>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    if let Some(focus) = focus.clone() {
        popup = popup.action_context(focus);
    }
    let scrollable = items.len() > 20;
    for item in items {
        popup = match item {
            OwnedMenuItem::Action { name, action, .. } => {
                if let Some(label) = name.strip_prefix("✓ ") {
                    popup.menu_with_check(label.to_owned(), true, action)
                } else {
                    popup.menu(name, action)
                }
            }
            OwnedMenuItem::Separator => popup.separator(),
            OwnedMenuItem::Submenu(menu) => {
                let focus = focus.clone();
                popup.submenu(menu.name, window, cx, move |popup, window, cx| {
                    fill_popup(popup, menu.items.clone(), focus.clone(), window, cx)
                })
            }
            OwnedMenuItem::SystemMenu(_) => popup,
        };
    }
    popup.scrollable(scrollable)
}

impl Render for WindowsMenuBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut bar = div()
            .id("windows-app-menu")
            .flex()
            .items_center()
            .size_full()
            .overflow_x_scroll()
            .children(self.menus.iter().enumerate().map(|(index, menu)| {
                let popup = self
                    .active
                    .as_ref()
                    .filter(|(active, _, _)| *active == index)
                    .map(|(_, popup, _)| popup.clone());
                let access = ['F', 'E', 'P', 'O', 'V', 'T', 'H'][index];
                div()
                    .id(index)
                    .relative()
                    .child(
                        Button::new("menu")
                            .small()
                            .compact()
                            .ghost()
                            .px_1p5()
                            .h(px(24.0))
                            .label(menu.name.clone())
                            .tooltip(format!("Alt+{access}"))
                            .selected(popup.is_some())
                            .tab_stop(false)
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(move |this, _, w, cx| this.toggle(index, w, cx))),
                    )
                    .on_hover(cx.listener(move |this, hovered: &bool, w, cx| {
                        if *hovered
                            && this
                                .active
                                .as_ref()
                                .is_some_and(|(active, _, _)| *active != index)
                        {
                            this.open(index, w, cx);
                        }
                    }))
                    .when_some(popup, |node, popup| {
                        node.child(deferred(
                            anchored()
                                .anchor(Corner::TopLeft)
                                .snap_to_window_with_margin(px(8.0))
                                .child(div().occlude().top_1().child(popup)),
                        ))
                    })
            }));
        for (action, forward) in self.switch_actions.iter().zip([false, true]) {
            let view = cx.weak_entity();
            bar = bar.on_boxed_action(action.as_ref(), move |_, window, cx| {
                let _ = view.update(cx, |this, cx| this.move_menu(forward, window, cx));
            });
        }
        bar
    }
}
