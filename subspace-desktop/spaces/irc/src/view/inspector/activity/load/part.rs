//! Reading one part of an item too large to hold whole, pinned to the
//! version of the part shown, within the room made for it (SUB decisions
//! `8357ec55` §2 and `33ccecf6` item 1). While the read is under way the
//! item counts the part shown and its drawn window, which still draw,
//! plus a whole page for the part coming in, so two reads never count on
//! the same room; once the part arrives it counts that part and its
//! window alone. The read goes ahead even when less than a page of room
//! is left: refusing would hide the item, and the budget's floor keeps
//! the part shown, its window and the page coming in.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{AppContext as _, Context};
use subspace_client_fleet::activity::{ItemPart, PartFailed};

use super::{Held, PAGE, Window, client, current, make_room, page_window, restamp};
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
    if !selection.loaded.contains_key(key) {
        return;
    }
    // Room for a whole page beside the one shown and its window, which it
    // replaces, held until the part arrives.
    make_room(selection, PAGE, Some(key));
    selection.opened += 1;
    let read = selection.opened;
    // The latest request from its admission: the one the floor keeps.
    selection.asked.insert(key.clone(), read);
    let agent = selection.agent.clone();
    let Some(loaded) = selection.loaded.get_mut(key) else {
        return;
    };
    let shown = loaded.raw.saturating_add(loaded.window.charge());
    let Held::Parts(parts) = &mut loaded.held else {
        return;
    };
    parts.loading = Some(page);
    parts.failed = None;
    loaded.bytes = shown.saturating_add(PAGE);
    // Only the version is pinned: no copy of the page outside its room.
    let pinned = parts.page.version();
    // The part read before, if any, asks for nothing more.
    loaded.cancel.store(true, Ordering::Release);
    let cancel = Arc::new(AtomicBool::new(false));
    let (read_key, flag) = (key.clone(), cancel.clone());
    loaded.cancel = cancel;
    loaded.opened = read;
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
/// window. A failed read is tried again at the page it names: page 0 once the
/// item is known to have changed.
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
    let data = &parts.page.data;
    (loaded.raw, loaded.window) = (data.len() as u64, Window::Whole(page_window(data)));
    loaded.bytes = loaded.raw.saturating_add(loaded.window.charge());
    if let Some(selection) = board.inspector_activity.as_mut() {
        restamp(selection, key);
        make_room(selection, 0, Some(key));
        super::super::draw::ask(selection, key);
    }
    cx.notify();
}
