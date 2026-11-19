//! The conversation a Back on another Space returns to, such as
//! the channel an issue chip was opened from. The conversation keeps its
//! panel and scroll position while the board is hidden, so showing it again
//! restores where the person was reading.

use gpui_kit::{App, Context};
use subspace_client_irc::{BufferKey, Target};
use subspace_framework::dashboard::ReturnLocation;

use crate::{Irc, view};

/// The conversation the person works in, as a place to return to.
pub(crate) fn location(board: &Irc) -> Option<ReturnLocation> {
    place(board.dock.focused.as_ref()?)
}

/// Conversation `buffer` as a place to return to.
pub(crate) fn place(buffer: &BufferKey) -> Option<ReturnLocation> {
    let label = match &buffer.target {
        Target::Channel(name) => name.clone(),
        Target::Query(nick) => nick.clone(),
        Target::Server => buffer.network.0.clone(),
    };
    Some(ReturnLocation {
        label: label.into(),
        key: serde_json::to_string(buffer).ok()?.into(),
    })
}

/// Shows the conversation `key` names again. False when it is gone, such
/// as a conversation closed since, or a channel left, parted or kicked
/// from since: its transcript may remain, but it is no longer joined.
pub(crate) fn show(board: &mut Irc, key: &str, cx: &mut Context<Irc>) -> bool {
    let Ok(buffer) = serde_json::from_str::<BufferKey>(key) else {
        return false;
    };
    if !exists(board, &buffer, cx) {
        return false;
    }
    view::reveal(board, &buffer, cx);
    true
}

fn exists(board: &Irc, buffer: &BufferKey, cx: &App) -> bool {
    let Some(hub) = board.hub.as_ref() else {
        return false;
    };
    let state = &hub.read(cx).state;
    let Some(found) = state.buffer(buffer) else {
        return false;
    };
    let joined = found.joined || !matches!(buffer.target, Target::Channel(_));
    joined && state.removed.get(buffer).is_none()
}
