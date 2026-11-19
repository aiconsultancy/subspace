//! The Join channel dialog: a channel name, an optional key, on the
//! selected network.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    KeyBinding, ParentElement as _, Styled as _, Subscription, Window, actions, div,
};
use subspace_client_irc::{Command, NetworkId};
use subspace_framework::dashboard::action::{Intent, dialog_button};
use subspace_framework::dashboard::dialog::{self, DialogWidth};
use subspace_framework::dashboard::fields::{self, FieldLayout, LabelledInput};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, rich};

use crate::Irc;

actions!(irc_join, [CancelJoin]);

const CONTEXT: &str = "IrcJoin";

pub(crate) struct Dialog {
    network: NetworkId,
    network_name: String,
    channel: Entity<InputState>,
    key: Entity<InputState>,
    error: Option<String>,
    previous_focus: Option<FocusHandle>,
    _subscriptions: Vec<Subscription>,
}

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CancelJoin, Some(CONTEXT))]);
}

/// The channel name to join: `#` is added when no channel prefix is typed.
pub(crate) fn channel_name(typed: &str) -> Option<String> {
    let typed = typed.trim();
    if typed.is_empty() || typed.contains([' ', ',', '\u{7}']) {
        return None;
    }
    Some(if typed.starts_with(['#', '&', '+', '!']) {
        typed.to_owned()
    } else {
        format!("#{typed}")
    })
}

pub(crate) fn open(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    let Some(network) = board.selected_network(cx) else {
        return;
    };
    let network_name = board
        .hub
        .as_ref()
        .and_then(|hub| {
            hub.read(cx)
                .state
                .network(&network)
                .map(|known| known.name.clone())
        })
        .unwrap_or_else(|| network.0.clone());
    let channel = cx.new(|cx| InputState::new(window, cx).placeholder("#channel"));
    let key = cx.new(|cx| InputState::new(window, cx).placeholder("Optional"));
    let subscriptions = [&channel, &key]
        .into_iter()
        .map(|input| {
            cx.subscribe_in(input, window, |board, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    submit(board, window, cx);
                }
            })
        })
        .collect();
    let previous_focus = window.focused(cx);
    channel.update(cx, |input, cx| input.focus(window, cx));
    board.join = Some(Dialog {
        network,
        network_name,
        channel,
        key,
        error: None,
        previous_focus,
        _subscriptions: subscriptions,
    });
    cx.notify();
}

pub(crate) fn close(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    if let Some(dialog) = board.join.take() {
        if let Some(focus) = dialog.previous_focus {
            focus.focus(window, cx);
        }
        cx.notify();
    }
}

fn submit(board: &mut Irc, window: &mut Window, cx: &mut Context<Irc>) {
    let Some(dialog) = board.join.as_mut() else {
        return;
    };
    let typed = dialog.channel.read(cx).value().to_string();
    let Some(channel) = channel_name(&typed) else {
        dialog.error = Some("Enter a channel name without spaces or commas.".into());
        cx.notify();
        return;
    };
    let key = dialog.key.read(cx).value().trim().to_owned();
    let command = Command::Join {
        network: dialog.network.clone(),
        channel,
        key: (!key.is_empty()).then_some(key),
    };
    board.send(command, cx);
    close(board, window, cx);
}

pub(crate) fn render(board: &Irc, cx: &mut Context<Irc>) -> Option<AnyElement> {
    let dialog = board.join.as_ref()?;
    let tokens = Tokens::current(cx);
    let frame = dialog::frame(&tokens, DialogWidth::D540, format!("Join a channel on {}", dialog.network_name))
        .key_context(CONTEXT)
        .on_action(cx.listener(|board, _: &CancelJoin, window, cx| close(board, window, cx)))
        .child(fields::labelled_input(
            LabelledInput::new(&dialog.channel, "Channel", FieldLayout::Stacked)
                .error(dialog.error.clone()),
            &tokens,
        ))
        .child(fields::labelled_input(
            LabelledInput::new(&dialog.key, "Channel key", FieldLayout::Stacked),
            &tokens,
        ))
        .child(div().text_size(tokens.caption_size()).text_color(tokens.muted).child(
            rich::plain_text(
                "irc-join-hint",
                "Joined channels last until you leave them or disconnect. Add a channel to the network's channels in Settings > Channels to join it on every connect.",
            ),
        ))
        .child(
            dialog::footer()
                .child(
                    dialog_button("irc-join-cancel", Intent::Neutral, Some(IconName::X), "Cancel")
                        .on_click(cx.listener(|board, _, window, cx| close(board, window, cx))),
                )
                .child(
                    dialog_button("irc-join-submit", Intent::Primary, Some(IconName::Hash), "Join")
                        .on_click(cx.listener(|board, _, window, cx| submit(board, window, cx))),
                ),
        );
    Some(dialog::backdrop(&tokens, frame))
}

#[cfg(test)]
mod tests {
    use super::channel_name;

    #[test]
    fn channel_names_gain_a_hash_and_reject_separators() {
        assert_eq!(channel_name(" subspace ").as_deref(), Some("#subspace"));
        assert_eq!(channel_name("&local").as_deref(), Some("&local"));
        assert_eq!(channel_name("two words"), None);
        assert_eq!(channel_name("a,b"), None);
        assert_eq!(channel_name(""), None);
    }
}
