use super::{Canvas, ChatPanel, Context};
use crate::action::{Intent, action_button, icon_button};
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{IntoElement as _, Pixels, Window, div};
use subspace_agent::{ChatCommand, Command};
use subspace_dashboard::{
    icon_button::IconButton,
    overflow_toolbar::{OverflowUnit, PopupEntry},
};
use subspace_lucide::IconName;

#[derive(Clone)]
pub(super) enum CanvasAction {
    Previous,
    Next,
    Zoom,
    Close(String),
    Hide,
}

impl CanvasAction {
    fn presentation(&self) -> (&'static str, IconName, &'static str) {
        match self {
            Self::Previous => ("canvas-prev", IconName::ChevronLeft, "Previous canvas"),
            Self::Next => ("canvas-next", IconName::ChevronRight, "Next canvas"),
            Self::Zoom => ("canvas-zoom", IconName::Maximize, "Zoom"),
            Self::Close(_) => ("canvas-close", IconName::X, "Close current canvas"),
            Self::Hide => ("canvas-hide", IconName::PanelRightClose, "Hide canvas area"),
        }
    }

    pub(super) fn activate(
        &self,
        chat: &mut ChatPanel,
        window: &mut Window,
        cx: &mut Context<ChatPanel>,
    ) {
        match self {
            Self::Previous => chat.tab_mut().canvases.step(-1),
            Self::Next => chat.tab_mut().canvases.step(1),
            Self::Zoom => chat.toggle_canvas_gallery(window, cx),
            Self::Close(id) => {
                chat.tab_mut().canvases.close(id);
                chat.send(
                    Command::Chat {
                        chat: chat.tab().id,
                        command: ChatCommand::CloseCanvas(id.clone()),
                    },
                    cx,
                );
                chat.input.update(cx, |input, cx| input.focus(window, cx));
            }
            Self::Hide => {
                chat.tab_mut().canvases.hidden = true;
                chat.tab_mut().canvases.gallery = false;
                chat.input.update(cx, |input, cx| input.focus(window, cx));
            }
        }
        cx.notify();
    }
}

pub(super) fn units(canvas: &Canvas, gallery: bool, cx: &Context<ChatPanel>) -> Vec<OverflowUnit> {
    [
        CanvasAction::Previous,
        CanvasAction::Next,
        CanvasAction::Zoom,
        CanvasAction::Close(canvas.id.clone()),
        CanvasAction::Hide,
    ]
    .into_iter()
    .map(|action| unit(action, gallery, cx))
    .collect()
}

fn unit(action: CanvasAction, gallery: bool, cx: &Context<ChatPanel>) -> OverflowUnit {
    let owner = cx.weak_entity();
    let popup_owner = owner.clone();
    let popup_action = action.clone();
    let (key, icon, name) = action.presentation();
    let label = if matches!(action, CanvasAction::Zoom) && gallery {
        "Close gallery"
    } else {
        name
    };
    let entry = PopupEntry::new(move || {
        let owner = popup_owner.clone();
        let action = popup_action.clone();
        PopupMenuItem::new(label)
            .icon(icon)
            .on_click(move |_, window, cx| {
                _ = owner.update(cx, |chat, cx| action.activate(chat, window, cx));
            })
    });
    OverflowUnit::inline(
        key,
        f32::from(Pixels::ZERO),
        move |_, cx| {
            let action = action.clone();
            owner
                .update(cx, |_, cx| {
                    button(&action, label)
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

fn button(action: &CanvasAction, label: &'static str) -> IconButton {
    let (key, icon, _) = action.presentation();
    if matches!(action, CanvasAction::Zoom) {
        action_button(key, Intent::Neutral, Some(icon), label)
    } else {
        icon_button(key, Intent::Quiet, icon, label)
    }
}
