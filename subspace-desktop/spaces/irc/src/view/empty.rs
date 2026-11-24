//! What the board shows before it can list anything: no network yet, not
//! running, or closed to someone who belongs to no initiative.

use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::{Tokens, feedback};
use subspace_framework::lucide::IconName;

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

/// Channels while the networks wait for the first read of the memberships.
pub(super) fn checking(cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    div()
        .id("irc-checking-initiatives")
        .test_support()
        .size_full()
        .bg(tokens.bg)
        .px(subspace_framework::dashboard::workspace::text_edge(&tokens))
        .py(tokens.pad_x)
        .child(feedback::loading_line(
            &tokens,
            "irc-checking-initiatives-line",
            subspace_api_client::initiatives::CHECKING,
        ))
        .into_any_element()
}

/// Channels with no network to show because the API that brings them
/// cannot be reached: why, in the words every Space uses, and **Reload**.
pub(super) fn unreachable(board: &Irc, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    h_flex()
        .id("irc-unreachable")
        .test_support()
        .size_full()
        .justify_center()
        .bg(tokens.bg)
        .child(feedback::empty_with_actions(
            &tokens,
            "irc-unreachable",
            "Offline",
            subspace_api_client::unreachable_message(),
            [reload_action(board, "irc-unreachable-reload")],
        ))
        .into_any_element()
}

/// **Reload**: reads the memberships again and reconnects what is idle, as
/// the Space's Reload does.
fn reload_action(
    board: &Irc,
    id: &'static str,
) -> subspace_framework::dashboard::icon_button::IconButton {
    let owner = board.me.clone();
    feedback::recovery_action(
        id,
        Intent::Primary,
        IconName::RotateCw,
        "Reload",
        move |_, window, cx| {
            _ = owner.update(cx, |board, cx| board.reload(window, cx));
        },
    )
}

/// Channels while the API says the person belongs to no initiative: no
/// network runs and nothing else shows until an administrator adds them.
/// **Reload** reads the memberships again.
pub(super) fn not_a_member(board: &Irc, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let reload = reload_action(board, "irc-no-initiative-reload");
    h_flex()
        .id("irc-no-initiative")
        .test_support()
        .size_full()
        .justify_center()
        .bg(tokens.bg)
        .child(feedback::empty_with_actions(
            &tokens,
            "irc-no-initiative",
            subspace_api_client::initiatives::NO_MEMBERSHIPS,
            format!(
                "Once an administrator adds you to an initiative in {}, reload to connect to its channels.",
                subspace_api_client::service_name()
            ),
            [reload],
        ))
        .into_any_element()
}
