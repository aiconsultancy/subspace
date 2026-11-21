//! The dock's own panel: the empty centre while no conversation is open.
//! The buffer list and the member list show in the workspace's navigator and
//! inspector (`view/frame.rs`).

use gpui_kit::component::dock::{BasePanel, Panel, PanelControl, PanelEvent};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, Context, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window,
};
use subspace_framework::dashboard::{Tokens, feedback};

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

/// A fixed panel: never closed or zoomed, no title bar.
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

fixed_panel!(EmptyPanel, "irc.empty", "Conversations");
