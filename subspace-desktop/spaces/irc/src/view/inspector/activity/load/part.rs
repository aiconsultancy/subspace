//! Reading one part of an item too large to hold whole, pinned to the
//! version of the part shown, within the room made for it (SUB decisions
//! `8357ec55` §2, `33ccecf6` item 1 and `4311830a` item 1). While the read
//! is under way the item counts the part shown and its drawn window, which
//! still draw, within its steady allowance, plus a whole page for the part
//! coming in, so two reads never count on the same room; once the part
//! arrives it counts that part and its window alone. The read goes ahead
//! even when less than a page of room is left: refusing would hide the
//! item, and the floor keeps a part and its window beside the page coming
//! in ([`super::super::ledger`]).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{AppContext as _, Context};
use subspace_client_fleet::activity::{ItemPart, PartFailed};

use super::{Held, PAGE, Reload, client, current, make_room, next_read, place, restamp, stamp};
use crate::Irc;

/// Shows part `page` of an item read part by part, pinned to the version
/// of the part shown.
pub(in super::super) fn show_page(
    board: &mut Irc,
    key: &(String, String),
    page: u32,
    cx: &mut Context<Irc>,
) {
    let client = board
        .inspector_activity
        .as_ref()
        .and_then(|selection| client(selection, cx));
    let Some((selection, client)) = board.inspector_activity.as_mut().zip(client) else {
        return;
    };
    let Some(loaded) = selection.loaded.get_mut(key) else {
        return;
    };
    if !matches!(loaded.held, Held::Parts(_)) {
        return;
    }
    // Room for a whole page beside the one shown and its window, which it
    // replaces, held until the part arrives.
    loaded.incoming = PAGE;
    // The latest request from its admission: the one the floor keeps.
    stamp(selection, key);
    let read = next_read(selection);
    make_room(selection, 0, Some(key));
    let agent = selection.agent.clone();
    let Some(loaded) = selection.loaded.get_mut(key) else {
        return;
    };
    let Held::Parts(parts) = &mut loaded.held else {
        return;
    };
    parts.loading = Some(page);
    parts.failed = None;
    // The latest request: any newer read of the item gives way to it.
    loaded.reload = Reload::None;
    // Only the version is pinned: no copy of the page outside its room.
    let pinned = parts.page.version();
    // The part read before, if any, asks for nothing more.
    loaded.cancel.store(true, Ordering::Release);
    let cancel = Arc::new(AtomicBool::new(false));
    let (read_key, flag) = (key.clone(), cancel.clone());
    loaded.cancel = cancel;
    loaded.read = read;
    loaded._read = Some(cx.spawn(async move |board, cx| {
        let (turn, item) = read_key.clone();
        let result = cx
            .background_spawn(async move {
                client.activity_item_part(&agent, &turn, &item, (page, Some(pinned)), &flag)
            })
            .await;
        let Some(result) = result else {
            return;
        };
        _ = board.update(cx, |board, cx| {
            publish_part(board, (&read_key, read), result, cx);
        });
    }));
    super::super::draw::replan(selection);
    cx.notify();
}

/// Shows the part read once it arrives, unless its item went or another
/// part was asked for meanwhile, and lets go of the part shown before, its
/// window and the page of room held for it, counting the new part and its
/// window. A failed read keeps the part shown, beside the error and **Try
/// again** for the page it names: page 0 once the item is known to have
/// changed.
fn publish_part(
    board: &mut Irc,
    (key, read): (&(String, String), u64),
    result: Result<ItemPart, PartFailed>,
    cx: &mut Context<Irc>,
) {
    let Some(loaded) = current(board, (key, read)) else {
        return;
    };
    let Held::Parts(parts) = &mut loaded.held else {
        return;
    };
    parts.loading = None;
    match result {
        Ok(part) => {
            parts.newer |= part.newer;
            parts.page = part.page;
        }
        Err(failed) => parts.failed = Some((failed.page, failed.error)),
    }
    loaded.raw = parts.page.data.len() as u64;
    loaded.incoming = 0;
    if let Some(selection) = board.inspector_activity.as_mut() {
        restamp(selection, key);
        place(selection, key);
    }
    cx.notify();
}
