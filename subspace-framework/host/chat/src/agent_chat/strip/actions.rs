use super::{ChatPanel, Context, IconName, Intent, icon_button};
use crate::action::action_button;
use gpui_kit::component::{Selectable as _, menu::PopupMenuItem};
use gpui_kit::{IntoElement as _, Pixels, Window, div};
use subspace_dashboard::{
    icon_button::IconButton,
    overflow_toolbar::{OverflowUnit, PopupEntry},
};

#[derive(Clone, Copy)]
enum Action {
    New,
    Sessions,
    Canvas,
    Close,
}

impl Action {
    fn presentation(self) -> (&'static str, IconName, &'static str) {
        match self {
            Self::New => ("chat-new-tab", IconName::Plus, "New chat (Cmd+T)"),
            Self::Sessions => ("chat-sessions", IconName::Grid2x2Plus, "Sessions"),
            Self::Canvas => ("canvas-show", IconName::PanelLeftOpen, "Show canvas area"),
            Self::Close => ("chat-close", IconName::X, "Close chat"),
        }
    }

    fn activate(self, chat: &mut ChatPanel, window: &mut Window, cx: &mut Context<ChatPanel>) {
        match self {
            Self::New => chat.new_tab(window, cx),
            Self::Sessions => chat.show_sessions(window, cx),
            Self::Canvas => {
                chat.tab_mut().canvases.hidden = false;
                cx.notify();
            }
            Self::Close => cx.emit(super::super::ChatPanelEvent::Close),
        }
    }
}

pub(super) fn units(chat: &ChatPanel, cx: &Context<ChatPanel>) -> Vec<OverflowUnit> {
    if chat.is_disabled() {
        return vec![unit(Action::Close, false, cx)];
    }
    let mut actions = vec![Action::New, Action::Sessions];
    let list = &chat.tab().canvases;
    if list.hidden && !list.items.is_empty() {
        actions.push(Action::Canvas);
    }
    actions.push(Action::Close);
    actions
        .into_iter()
        .map(|action| unit(action, chat.sessions_open, cx))
        .collect()
}

fn unit(action: Action, sessions_open: bool, cx: &Context<ChatPanel>) -> OverflowUnit {
    let owner = cx.weak_entity();
    let popup_owner = owner.clone();
    let (key, icon, label) = action.presentation();
    let entry = PopupEntry::new(move || {
        let owner = popup_owner.clone();
        PopupMenuItem::new(label)
            .icon(icon)
            .checked(matches!(action, Action::Sessions) && sessions_open)
            .on_click(move |_, window, cx| {
                _ = owner.update(cx, |chat, cx| action.activate(chat, window, cx));
            })
    });
    OverflowUnit::inline(
        key,
        f32::from(Pixels::ZERO),
        move |_, cx| {
            owner
                .update(cx, |chat, cx| {
                    button(action, chat)
                        .on_click(
                            cx.listener(move |chat, _, window, cx| {
                                action.activate(chat, window, cx)
                            }),
                        )
                        .into_any_element()
                })
                .unwrap_or_else(|_| div().into_any_element())
        },
        vec![entry],
    )
}

fn button(action: Action, chat: &ChatPanel) -> IconButton {
    let (key, icon, label) = action.presentation();
    let button = if matches!(action, Action::Canvas) {
        action_button(
            key,
            Intent::Neutral,
            Some(icon),
            format!("Canvas ({})", chat.tab().canvases.items.len()),
        )
    } else {
        icon_button(key, Intent::Quiet, icon, label)
    };
    button.selected(matches!(action, Action::Sessions) && chat.sessions_open)
}
