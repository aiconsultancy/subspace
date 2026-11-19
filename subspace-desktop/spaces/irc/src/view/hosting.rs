//! Views the board keeps per buffer, and opening a channel named in a
//! message.

use gpui_kit::{Context, Entity, Subscription};
use subspace_client_irc::{Command, NetworkId, Target};

use super::reveal;
use crate::Irc;

/// A view the board keeps for one buffer, with its event subscription.
pub(crate) struct Hosted<V> {
    pub entity: Entity<V>,
    /// Kept so the events keep arriving.
    pub _subscription: Subscription,
}

/// Shows channel `name` on `network` when it is in the list, else joins it.
pub(crate) fn open_channel(
    board: &mut Irc,
    network: &NetworkId,
    name: &str,
    cx: &mut Context<Irc>,
) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    let known = hub
        .read(cx)
        .state
        .buffers()
        .map(|(key, _)| key)
        .find(|key| {
            &key.network == network
                && matches!(&key.target, Target::Channel(channel) if channel.eq_ignore_ascii_case(name))
        })
        .cloned();
    match known {
        Some(buffer) => reveal(board, &buffer, cx),
        None => board.send(
            Command::Join {
                network: network.clone(),
                channel: name.to_owned(),
                key: None,
            },
            cx,
        ),
    }
}
