//! Opening a stored message from search: its buffer's panel, scrolled to the
//! line and selected. A line the buffer does not hold, such as one older than
//! what was restored, is asked for around its time first.

use gpui_kit::Context;
use subspace_client_irc::BufferKey;
use subspace_framework::dashboard::full_text::{Hit, ReadPermit};
use subspace_framework::dashboard::{DashboardEvent, Toast, full_text_service};

use crate::{Irc, dock, store, view};

const GONE: &str = "This message is no longer on the server.";

/// A search result waiting for its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pending {
    pub buffer: BufferKey,
    /// The record id of the line.
    pub id: String,
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
    });
    if !settled(board, cx) {
        hub.update(cx, |hub, _| hub.load_around(&navigation));
    }
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
        cx.emit(DashboardEvent::Toast(Toast::info(GONE)));
    }
}

/// Scrolls to the waiting line when its buffer holds it. True when it did.
fn settled(board: &mut Irc, cx: &mut Context<Irc>) -> bool {
    let Some(pending) = board.reveal.clone() else {
        return true;
    };
    let Some(seq) = board.hub.as_ref().and_then(|hub| {
        hub.read(cx)
            .state
            .buffer(&pending.buffer)?
            .entries
            .iter()
            .find(|entry| store::document_id(&entry.line) == pending.id)
            .map(|entry| entry.seq)
    }) else {
        return false;
    };
    board.reveal = None;
    dock::with_window(board, cx, move |board, _, cx| {
        if let Some(hosted) = board.dock.open.get(&pending.buffer) {
            hosted
                .entity
                .update(cx, |conversation, cx| conversation.reveal(seq, cx));
        }
    });
    true
}
