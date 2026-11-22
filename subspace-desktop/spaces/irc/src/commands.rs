//! Palette commands for the network and buffer the toolbar acts on: the
//! toolbar's and row menu's actions, reachable from ⌘K.

use gpui_kit::{App, Context, WeakEntity, Window};
use subspace_client_irc::{BufferKey, Command, NetworkId};
use subspace_framework::dashboard::PaletteCommand;

use crate::remove::{self, Removal};
use crate::state::IrcState;
use crate::{Irc, join, listing, toolbar};

/// One command the palette offers, before it is bound to the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Offer {
    Join,
    Refresh(NetworkId),
    Connect(NetworkId, String),
    Disconnect(NetworkId, String),
    MarkRead(BufferKey, String),
    Remove(Removal, BufferKey),
}

/// What applies to the selected `network` and `buffer` as `state` holds them.
pub(crate) fn offers(
    state: &IrcState,
    network: Option<&NetworkId>,
    buffer: Option<&BufferKey>,
) -> Vec<Offer> {
    let mut offers = Vec::new();
    if let Some(known) = network.and_then(|id| state.network(id)) {
        if toolbar::join_unavailable(Some(&known.status)).is_none() {
            offers.push(Offer::Join);
            offers.push(Offer::Refresh(known.id.clone()));
        }
        let name = known.name.clone();
        offers.push(match toolbar::is_online(&known.status) {
            true => Offer::Disconnect(known.id.clone(), name),
            false => Offer::Connect(known.id.clone(), name),
        });
    }
    let Some(buffer) = buffer else {
        return offers;
    };
    let title = crate::rows::title(buffer).to_owned();
    if state.buffer(buffer).is_some_and(|held| held.unread > 0) {
        offers.push(Offer::MarkRead(buffer.clone(), title));
    }
    if let Some(removal) = Removal::now(state, buffer).filter(|_| remove::available(state, buffer))
    {
        offers.push(Offer::Remove(removal, buffer.clone()));
    }
    offers
}

/// The palette commands for what the toolbar acts on: the selection, or,
/// while the navigator is hidden, the conversation the toolbar names, so a
/// command such as Join channel… offers and opens on the same network.
pub(crate) fn commands(board: &Irc, cx: &Context<Irc>) -> Vec<PaletteCommand> {
    let Some(hub) = board.hub.as_ref() else {
        return Vec::new();
    };
    let network = board.toolbar_network(cx);
    let offers = offers(
        &hub.read(cx).state,
        network.as_ref(),
        board.toolbar_buffer(),
    );
    offers
        .into_iter()
        .map(|offer| command(board.me.clone(), offer))
        .collect()
}

fn command(board: WeakEntity<Irc>, offer: Offer) -> PaletteCommand {
    let label = match &offer {
        Offer::Join => "Join channel…".to_owned(),
        Offer::Refresh(_) => listing::REFRESH.to_owned(),
        Offer::Connect(_, name) => format!("Connect to {name}"),
        Offer::Disconnect(_, name) => format!("Disconnect from {name}"),
        Offer::MarkRead(_, title) => format!("Mark {title} as read"),
        Offer::Remove(removal, buffer) => {
            format!("{} {}", removal.label(), crate::rows::title(buffer))
        }
    };
    PaletteCommand::callback(label, move |window, cx| run(&board, &offer, window, cx))
        .keywords("irc channel network conversation")
}

fn run(board: &WeakEntity<Irc>, offer: &Offer, window: &mut Window, cx: &mut App) {
    _ = board.update(cx, |board, cx| match offer {
        Offer::Join => join::open(board, window, cx),
        Offer::Refresh(network) => listing::refresh(board, network, cx),
        Offer::Connect(network, _) => board.send(Command::Connect(network.clone()), cx),
        Offer::Disconnect(network, _) => board.send(
            Command::Disconnect {
                network: network.clone(),
                reason: String::new(),
            },
            cx,
        ),
        Offer::MarkRead(buffer, _) => {
            if let Some(hub) = board.hub.clone() {
                hub.update(cx, |hub, cx| hub.mark_buffer_read(buffer, cx));
            }
        }
        Offer::Remove(_, buffer) => remove::remove(board, buffer, cx),
    });
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::{ConnectionStatus, Event};

    use super::{Offer, offers};
    use crate::fixtures::{at, channel, config, message, network_id};
    use crate::remove::Removal;
    use crate::state::IrcState;

    #[test]
    fn the_palette_offers_what_the_selection_allows() {
        let mut state = IrcState::new(&[config()], Vec::new());
        let room = channel("#subspace");
        let offline = offers(&state, Some(&network_id()), None);
        assert_eq!(offline, [Offer::Connect(network_id(), "Local Ergo".into())]);
        state.apply(
            Event::Status {
                network: network_id(),
                status: ConnectionStatus::Connected {
                    nick: "subspace".into(),
                },
            },
            at(0),
        );
        state.apply(
            Event::Joined {
                buffer: room.clone(),
            },
            at(0),
        );
        state.apply(Event::Line(message(&room, "alice", "hi", 1)), at(1));
        let online = offers(&state, Some(&network_id()), Some(&room));
        assert_eq!(
            online,
            [
                Offer::Join,
                Offer::Refresh(network_id()),
                Offer::Disconnect(network_id(), "Local Ergo".into()),
                Offer::MarkRead(room.clone(), "#subspace".into()),
                Offer::Remove(Removal::Leave, room),
            ]
        );
    }
}
