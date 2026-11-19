//! The board before any network is configured, and the setup actions it
//! shares with Settings > Channels.

use gpui_kit::{
    AnyElement, App, ElementId, IntoElement as _, ParentElement as _, Styled as _, div,
};
use subspace_framework::dashboard::action::Intent;
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::{Tokens, copy_details, feedback, reveal, rich};
use subspace_framework::lucide::IconName;

use crate::config;

pub(crate) const TITLE: &str = "No IRC networks";
pub(crate) const HINT: &str =
    "Add a network in Settings, or copy a setup prompt for a coding agent.";

/// Copies the agent prompt for this computer's IRC settings file.
pub(crate) fn copy_prompt_button(id: impl Into<ElementId>) -> IconButton {
    copy_details::copy_text(
        id,
        "Copy agent prompt",
        "Agent prompt copied",
        |cx: &App| {
            crate::prompt::agent_prompt(&crate::prompt::Files {
                board: config::file(cx).unwrap_or_default(),
                product: config::product_file(cx).unwrap_or_default(),
            })
        },
    )
}

/// The empty state: what to do, the setup actions, and any settings error.
pub(crate) fn unconfigured(tokens: &Tokens, error: Option<String>, cx: &App) -> AnyElement {
    let add = feedback::recovery_action(
        "irc-add-network",
        Intent::Neutral,
        IconName::Plus,
        "Add network…",
        |_, window, cx| {
            window.dispatch_action(Box::new(subspace_framework::dashboard::ConfigureBoard), cx)
        },
    );
    let actions = [
        add.into_any_element(),
        copy_prompt_button("irc-copy-prompt").into_any_element(),
    ]
    .into_iter()
    .chain(
        config::file(cx).map(|path| reveal::button("irc-reveal-config", path).into_any_element()),
    );
    feedback::empty_with_actions(tokens, "irc-unconfigured", TITLE, HINT, actions)
        .children(error.map(|error| {
            div().text_color(tokens.red).child(rich::plain_text(
                "irc-settings-error",
                &format!("Some networks could not be read: {error}"),
            ))
        }))
        .into_any_element()
}
