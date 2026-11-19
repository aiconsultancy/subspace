//! What the board shows before it can list anything: no network yet, or
//! not running.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::{Tokens, feedback};

use crate::{Irc, setup};

/// The empty state when no network is configured: what to add and where.
pub(super) fn unconfigured(board: &Irc, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let error = board
        .hub
        .as_ref()
        .and_then(|hub| hub.read(cx).settings_error());
    div()
        .id("irc-unconfigured")
        .test_support()
        .size_full()
        .overflow_y_scroll()
        .bg(tokens.bg)
        .text_color(tokens.fg)
        .child(setup::unconfigured(&tokens, error, cx))
        .into_any_element()
}

/// The board before its background owner started, such as when Settings
/// turned it off.
pub(super) fn stopped(cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    h_flex()
        .size_full()
        .justify_center()
        .bg(tokens.bg)
        .child(feedback::empty(
            &tokens,
            "irc-stopped",
            "Channels is not running",
            "Enable the board in Settings to connect.",
        ))
        .into_any_element()
}
