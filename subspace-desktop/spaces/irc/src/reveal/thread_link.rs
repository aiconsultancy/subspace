//! Thread links resolve on the source network. Ergo serves older history
//! after the recipient has joined the target channel.

use std::time::Duration;

use gpui_kit::{Context, Window};
use subspace_client_irc::{BufferKey, Command, NetworkId, Target};
use subspace_framework::dashboard::{DashboardEvent, Toast};

use super::{JOIN_REFUSED, Loading, Pending, settle, settled};
use crate::{Irc, dock, store, view};

const JOIN_TIMEOUT: Duration = Duration::from_secs(12);

/// Opens a thread link on the source message's IRC network.
pub(crate) fn open_thread(
    board: &mut Irc,
    (network, channel): (&NetworkId, &str),
    id: &str,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    let state = &hub.read(cx).state;
    if state.network(network).is_none() {
        return;
    }
    let held = state.buffers().find_map(|(key, _)| {
        let same_channel =
            matches!(&key.target, Target::Channel(name) if name.eq_ignore_ascii_case(channel));
        (&key.network == network && same_channel).then(|| key.clone())
    });
    let open_after_load = held.is_none();
    let buffer = held.unwrap_or_else(|| {
        let name = crate::rows::unjoined(state, network)
            .into_iter()
            .find(|listed| listed.name.eq_ignore_ascii_case(channel))
            .map_or(channel, |listed| listed.name.as_str());
        BufferKey {
            network: network.clone(),
            target: Target::Channel(name.to_owned()),
        }
    });
    let needs_join = !state.buffer(&buffer).is_some_and(|held| held.joined)
        && crate::listing::connected(state, network);
    if !open_after_load {
        show_conversation(board, &buffer, window, cx);
    }
    if board.comms.active {
        board.comms.selected = Some(crate::comms::Key {
            buffer: buffer.clone(),
            root: id.to_owned(),
        });
    }
    let pending = Pending {
        buffer,
        id: id.to_owned(),
        thread: true,
        open_after_load,
        loading: if needs_join {
            Loading::Join
        } else {
            Loading::Queued
        },
    };
    let joining = board.reveal.as_ref().is_some_and(|waiting| {
        waiting.buffer == pending.buffer && waiting.loading == Loading::Join
    });
    board.reveal = Some(pending.clone());
    if settled(board, cx) {
        return;
    }
    if needs_join {
        join_channel(&hub, pending, joining, cx);
    } else {
        settle(board, cx);
    }
}

fn join_channel(
    hub: &gpui_kit::Entity<crate::hub::Hub>,
    pending: Pending,
    joining: bool,
    cx: &mut Context<Irc>,
) {
    let Target::Channel(channel) = &pending.buffer.target else {
        return;
    };
    if !joining {
        hub.update(cx, |hub, _| {
            hub.send(Command::Join {
                network: pending.buffer.network.clone(),
                channel: channel.clone(),
                key: hub.join_key(&pending.buffer.network, channel),
            });
        });
    }
    cx.spawn(async move |board, cx| {
        cx.background_executor().timer(JOIN_TIMEOUT).await;
        _ = board.update(cx, |board, cx| {
            if board.reveal.as_ref() == Some(&pending) {
                board.reveal = None;
                cx.emit(DashboardEvent::Toast(Toast::info(JOIN_REFUSED)));
                cx.notify();
            }
        });
    })
    .detach();
}

pub(super) fn show_conversation(
    board: &mut Irc,
    buffer: &BufferKey,
    window: &mut Window,
    cx: &mut Context<Irc>,
) {
    if !board.comms.active {
        view::reveal_row(board, buffer, cx);
    }
    dock::open(board, buffer, dock::Where::Tab, window, cx);
}

pub(super) fn request_thread(hub: &mut crate::hub::Hub, pending: &Pending) {
    hub.load_around(&store::Navigation {
        network: pending.buffer.network.clone(),
        target: pending.buffer.target.clone(),
        msgid: Some(pending.id.clone()),
        time: chrono::Utc::now(),
        from: None,
    });
}
