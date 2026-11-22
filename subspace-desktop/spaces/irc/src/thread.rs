//! The inspector's **Thread** tab in a ship's channel or a Starbase: one
//! thread at a time, being a message that has replies or a request (a
//! message addressed to an agent). It shows the root message, a request's
//! status timeline, then the replies. The transcript is unchanged: replies
//! still show in the channel, and this is another view of them.
//!
//! - **Which messages belong** ([`model`]): a reply is a message whose
//!   server reply tag (`+draft/reply`) leads, through held parents, to the
//!   root, read by the same [`subspace_client_irc::ThreadRoots`] as the
//!   transcript's reply counts. Nothing in a message's text places it in a
//!   thread.
//! - **The timeline** comes only from the progress notices the status line
//!   folds ([`crate::thread_status`]): a notice folds when its server
//!   account equals the nick the held request asks. An untrusted notice
//!   stays an ordinary reply and never makes a step.
//! - **The chosen thread** ([`chosen`]) is kept per conversation for the
//!   session, rebuilt only when its buffer changed, and let go once its
//!   root is no longer held.
//! - **Replying** goes to the channel with the reply tag
//!   ([`crate::compose::reply`]); nothing here sends a private message.

mod chosen;
mod model;
#[cfg(test)]
mod tests;

use gpui_kit::{Context, Window};
use subspace_client_irc::{BufferKey, ConnectionStatus, Target};

pub(crate) use chosen::Threads;
#[cfg(test)]
use model::build;
pub(crate) use model::{Step, StepKind, Thread, Timeline};

use crate::Irc;
use crate::fleet::{ChannelKind, Roster};
use crate::state::Network;

/// The capability the server's reply tag needs.
pub(crate) const MESSAGE_TAGS: &str = "message-tags";

/// Whether channel `buffer` has the inspector's tabs, and so a Thread tab:
/// a ship's channel or a Starbase the roster knows.
pub(crate) fn has_tab(roster: &Roster, buffer: &BufferKey) -> bool {
    match &buffer.target {
        Target::Channel(name) => matches!(
            roster.channel(name),
            Some(ChannelKind::Ship { .. } | ChannelKind::Starbase)
        ),
        Target::Query(_) | Target::Server => false,
    }
}

/// Why a reply cannot be sent on `network` now; `None` when it can: the
/// connection is up and agreed `message-tags`, which carries the reply tag.
pub(crate) fn reply_refusal(network: Option<&Network>) -> Option<&'static str> {
    let Some(network) = network else {
        return Some(NOT_CONNECTED);
    };
    if !matches!(network.status, ConnectionStatus::Connected { .. }) {
        return Some(NOT_CONNECTED);
    }
    let tags = network.caps.iter().any(|cap| cap == MESSAGE_TAGS);
    (!tags).then_some(NO_REPLY_TAG)
}

/// Why replying waits for the network.
pub(crate) const NOT_CONNECTED: &str = "Replies can be sent once the network is connected.";
/// Why replying is not offered on a server without the reply tag.
pub(crate) const NO_REPLY_TAG: &str =
    "This server does not carry reply tags (message-tags), so a reply can't join the thread.";

/// Whether each reply continues the group of the one before it, by the
/// transcript's rule ([`crate::lines::continues`]): the first never does.
pub(crate) fn continues(replies: &[crate::state::Entry]) -> Vec<bool> {
    std::iter::once(false)
        .chain(
            replies
                .windows(2)
                .map(|pair| crate::lines::continues(&pair[0], &pair[1])),
        )
        .take(replies.len())
        .collect()
}

/// Opens the thread of message `root` in `buffer`: the inspector shows,
/// on its Thread tab.
pub(crate) fn open(
    board: &mut Irc,
    buffer: &BufferKey,
    root: String,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    let previous = board.threads.chosen(buffer).map(str::to_owned);
    board.threads.choose(buffer, root.clone());
    if previous.is_some_and(|previous| previous != root) {
        cancel_reply(board, buffer, window, cx);
    }
    refresh(board, cx);
    board.inspector_tab = crate::view::InspectorTab::Thread;
    if !board.dock.members_open() {
        crate::dock::toggle_members(board, cx);
    }
    cx.notify();
}

/// Rebuilds each chosen thread whose buffer changed, and lets go of those
/// whose root is no longer held, with any reply being written to it.
pub(crate) fn refresh(board: &mut Irc, cx: &mut Context<Irc>) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    let gone = board.threads.refresh(&hub.read(cx).state);
    if gone.is_empty() {
        return;
    }
    crate::dock::with_window(board, cx, move |board, window, cx| {
        for (buffer, root) in &gone {
            if let Some(hosted) = board.dock.open.get(buffer) {
                hosted.entity.update(cx, |view, cx| {
                    crate::compose::reply::forget(view, root, window, cx);
                });
            }
        }
    });
}

/// Leaves reply mode in `buffer`'s composer.
fn cancel_reply(board: &Irc, buffer: &BufferKey, window: &mut Window, cx: &mut Context<Irc>) {
    if let Some(hosted) = board.dock.open.get(buffer) {
        hosted.entity.update(cx, |view, cx| {
            crate::compose::reply::cancel(view, window, cx)
        });
    }
}
