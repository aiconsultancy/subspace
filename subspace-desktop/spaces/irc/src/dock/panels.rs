//! The dock's own panels: the buffer list on the left, the member list on
//! the right, and the empty centre while no conversation is open.

use gpui_kit::component::dock::{BasePanel, Panel, PanelControl, PanelEvent};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    TestSupportExt as _, WeakEntity, Window, div,
};
use subspace_framework::dashboard::{Tokens, feedback, list};

use crate::Irc;
use crate::members::MemberList;

/// The networks and their buffers, drawn by the board's shared list.
pub(crate) struct BuffersPanel {
    owner: WeakEntity<Irc>,
    focus: FocusHandle,
}

impl BuffersPanel {
    pub fn new(owner: WeakEntity<Irc>, cx: &mut Context<Self>) -> Self {
        Self {
            owner,
            focus: cx.focus_handle(),
        }
    }
}

impl Render for BuffersPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self
            .owner
            .update(cx, |board, cx| {
                list::render(board, window, cx)
                    .size_full()
                    .into_any_element()
            })
            .unwrap_or_else(|_| div().into_any_element());
        side_pane("irc-buffers-pane", content)
    }
}

/// The focused channel's member list, or a hint for other buffers.
pub(crate) struct MembersPanel {
    list: Option<Entity<MemberList>>,
    focus: FocusHandle,
}

impl MembersPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            list: None,
            focus: cx.focus_handle(),
        }
    }

    /// Shows `list`, the member list of the focused channel, or the hint.
    pub fn set_list(&mut self, list: Option<Entity<MemberList>>, cx: &mut Context<Self>) {
        if self.list != list {
            self.list = list;
            cx.notify();
        }
    }
}

impl Render for MembersPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.list {
            Some(list) => list.clone().into_any_element(),
            None => hint(
                cx,
                "irc-members-none",
                "No channel",
                "Open a channel to see its members.",
            ),
        };
        side_pane("irc-members-pane", content)
    }
}

fn side_pane(id: &'static str, content: AnyElement) -> impl IntoElement {
    div().id(id).test_support().size_full().child(content)
}

/// The centre while no conversation is open.
pub(crate) struct EmptyPanel {
    focus: FocusHandle,
}

impl EmptyPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
}

impl Render for EmptyPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        hint(
            cx,
            "irc-nothing-open",
            "No conversation open",
            "Choose a channel or private conversation in the list.",
        )
    }
}

fn hint(cx: &App, id: &'static str, title: &'static str, text: &'static str) -> AnyElement {
    let tokens = Tokens::current(cx);
    h_flex()
        .size_full()
        .justify_center()
        .bg(tokens.surface)
        .child(feedback::empty(&tokens, id, title, text))
        .into_any_element()
}

/// The fixed side and centre panels: never closed or zoomed, no title bar.
macro_rules! fixed_panel {
    ($panel:ty, $name:literal, $tab:literal) => {
        impl EventEmitter<PanelEvent> for $panel {}

        impl Focusable for $panel {
            fn focus_handle(&self, _: &App) -> FocusHandle {
                self.focus.clone()
            }
        }

        impl BasePanel for $panel {
            fn panel_name(&self) -> &'static str {
                $name
            }

            fn closable(&self, _: &App) -> bool {
                false
            }

            fn zoomable(&self, _: &App) -> bool {
                false
            }
        }

        impl Panel for $panel {
            fn tab_name(&self, _: &App) -> Option<SharedString> {
                Some($tab.into())
            }

            fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                $tab
            }

            fn zoom_control(&self, _: &App) -> Option<PanelControl> {
                None
            }

            fn inner_padding(&self, _: &App) -> bool {
                false
            }

            fn title_bar(&self, _: &App) -> bool {
                false
            }
        }
    };
}

fixed_panel!(BuffersPanel, "irc.buffers", "Buffers");
fixed_panel!(MembersPanel, "irc.members", "Members");
fixed_panel!(EmptyPanel, "irc.empty", "Conversations");
