//! Opening a stored message from search: its buffer's panel, scrolled to the
//! line and selected. A line the buffer does not hold, such as one older than
//! what was restored, is asked for around its time first.

use gpui_kit::{Context, Window};
use subspace_client_irc::{BufferKey, NetworkId, Target, ThreadRoots};
use subspace_framework::dashboard::full_text::{Hit, ReadPermit};
use subspace_framework::dashboard::{DashboardEvent, Toast, full_text_service};

use crate::{Irc, dock, store, view};

const GONE: &str = "This message is no longer on the server.";

/// A search result waiting for its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pending {
    pub buffer: BufferKey,
    pub id: String,
    /// Thread links name the IRC message ID. Search names the stored record.
    pub thread: bool,
    open_after_load: bool,
}

/// Opens `hit` when it is a valid, current IRC record of this app.
pub(crate) fn open(board: &mut Irc, hit: &Hit, permit: &ReadPermit, cx: &mut Context<Irc>) {
    let Some(navigation) = checked(hit, permit, cx) else {
        return;
    };
    let Some(hub) = board.hub.clone() else {
        return;
    };
    let buffer = navigation.buffer();
    let held = hub.update(cx, |hub, cx| {
        let held = hub.state.hold(&buffer);
        cx.notify();
        held
    });
    if !held {
        return;
    }
    board.active = true;
    view::sync(board, cx);
    view::reveal(board, &buffer, cx);
    board.reveal = Some(Pending {
        buffer,
        id: hit.document.id.clone(),
        thread: false,
        open_after_load: false,
    });
    if !settled(board, cx) {
        hub.update(cx, |hub, _| hub.load_around(&navigation));
    }
}

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
    if !open_after_load {
        show_conversation(board, &buffer, window, cx);
    }
    if board.comms.active {
        board.comms.selected = Some(crate::comms::Key {
            buffer: buffer.clone(),
            root: id.to_owned(),
        });
    }
    board.reveal = Some(Pending {
        buffer: buffer.clone(),
        id: id.to_owned(),
        thread: true,
        open_after_load,
    });
    if !settled(board, cx) {
        request_thread(board, &buffer, id, cx);
    }
}

fn show_conversation(
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

fn request_thread(board: &mut Irc, buffer: &BufferKey, id: &str, cx: &mut Context<Irc>) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    if !crate::listing::connected(&hub.read(cx).state, &buffer.network) {
        settle(board, cx);
        return;
    }
    let navigation = store::Navigation {
        network: buffer.network.clone(),
        target: buffer.target.clone(),
        msgid: Some(id.to_owned()),
        time: chrono::Utc::now(),
        from: None,
    };
    hub.update(cx, |hub, _| hub.load_around(&navigation));
}

fn supports_threads(board: &Irc, buffer: &BufferKey, cx: &Context<Irc>) -> bool {
    board.hub.as_ref().is_some_and(|hub| {
        let roster = hub.read(cx).roster(&buffer.network, cx);
        crate::thread::has_tab(&roster, buffer)
    })
}

/// The navigation of `hit`, when its permit holds and it is this app's IRC
/// record.
fn checked(hit: &Hit, permit: &ReadPermit, cx: &Context<Irc>) -> Option<store::Navigation> {
    let service = full_text_service(cx)?;
    let ours = permit.valid()
        && hit.target.scope.board == crate::ID
        && hit.target.scope.app == service.app_id()
        && hit.target.id == hit.document.id;
    ours.then(|| store::navigation(&hit.document)).flatten()
}

/// After the hub changed: scrolls to the waiting line once it is held, or
/// says it is gone once the server answered without it.
pub(crate) fn settle(board: &mut Irc, cx: &mut Context<Irc>) {
    let Some(pending) = board.reveal.clone() else {
        return;
    };
    if settled(board, cx) {
        return;
    }
    let waiting = board
        .hub
        .as_ref()
        .is_some_and(|hub| hub.read(cx).recall.awaiting(&pending.buffer));
    if !waiting {
        board.reveal = None;
        if !pending.thread || !open_retained_replies(board, &pending, cx) {
            cx.emit(DashboardEvent::Toast(Toast::info(GONE)));
        }
    }
}

/// Scrolls to the waiting line when its buffer holds it. True when it did.
fn settled(board: &mut Irc, cx: &mut Context<Irc>) -> bool {
    let Some(pending) = board.reveal.clone() else {
        return true;
    };
    let Some((seq, id, reply)) = board.hub.as_ref().and_then(|hub| {
        let line = hub
            .read(cx)
            .state
            .buffer(&pending.buffer)?
            .entries
            .iter()
            .find(|entry| {
                if pending.thread {
                    entry.line.id.as_deref() == Some(pending.id.as_str())
                } else {
                    store::document_id(&entry.line) == pending.id
                }
            })?;
        Some((line.seq, line.line.id.clone(), line.line.reply_to.is_some()))
    }) else {
        return false;
    };
    board.reveal = None;
    let in_thread = pending.thread || reply && supports_threads(board, &pending.buffer, cx);
    dock::with_window(board, cx, move |board, window, cx| {
        if in_thread && let Some(id) = id {
            if pending.open_after_load {
                show_conversation(board, &pending.buffer, window, cx);
            }
            if board.dock.focused.as_ref() == Some(&pending.buffer) {
                crate::thread::open(board, &pending.buffer, id, window, cx);
            }
        } else if let Some(hosted) = board.dock.open.get(&pending.buffer) {
            hosted
                .entity
                .update(cx, |conversation, cx| conversation.reveal(seq, cx));
        }
    });
    true
}

/// The original expired or the server couldn't load it. Its retained replies
/// can still open together using one of their message IDs.
fn open_retained_replies(board: &mut Irc, pending: &Pending, cx: &mut Context<Irc>) -> bool {
    let Some(id) = board.hub.as_ref().and_then(|hub| {
        let held = hub.read(cx).state.buffer(&pending.buffer)?;
        let lines = held.entries.iter().map(|entry| &entry.line);
        let roots = ThreadRoots::new(&pending.buffer, lines.clone());
        lines
            .filter(|line| roots.root(line) == Some(pending.id.as_str()))
            .find_map(|line| line.id.clone())
    }) else {
        return false;
    };
    let buffer = pending.buffer.clone();
    let open_after_load = pending.open_after_load;
    dock::with_window(board, cx, move |board, window, cx| {
        if open_after_load {
            show_conversation(board, &buffer, window, cx);
        }
        if board.dock.focused.as_ref() == Some(&buffer) {
            crate::thread::open(board, &buffer, id, window, cx);
        }
    });
    true
}
