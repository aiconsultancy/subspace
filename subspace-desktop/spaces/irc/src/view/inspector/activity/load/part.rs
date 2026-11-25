//! Reading one part of an item too large to hold whole, pinned to the
//! version of the part shown, within the room made for it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{AppContext as _, Context};
use subspace_client_fleet::Error;
use subspace_client_fleet::activity::ItemPart;

use super::{Held, PAGE, client, current, make_room};
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
    // Room for a whole page beside the one shown, which it replaces.
    make_room(selection, PAGE, Some(key));
    selection.opened += 1;
    let read = selection.opened;
    let agent = selection.agent.clone();
    let Some(loaded) = selection.loaded.get_mut(key) else {
        return;
    };
    let Held::Parts(parts) = &mut loaded.held else {
        return;
    };
    parts.loading = Some(page);
    parts.failed = None;
    let pinned = parts.page.clone();
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
                client.activity_item_part(&agent, &turn, &item, (page, Some(&pinned)), &flag)
            })
            .await;
        let Some(result) = result else {
            return;
        };
        _ = board.update(cx, |board, cx| {
            publish_part(board, (&read_key, read), (page, result), cx);
        });
    }));
    cx.notify();
}

/// Shows part `page` once read, unless its item went or another part was
/// asked for meanwhile.
fn publish_part(
    board: &mut Irc,
    (key, read): (&(String, String), u64),
    (page, result): (u32, Result<ItemPart, Error>),
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
            loaded.bytes = part.page.data.len() as u64;
            parts.newer |= part.newer;
            parts.page = part.page;
        }
        Err(error) => parts.failed = Some((page, error)),
    }
    if let Some(selection) = board.inspector_activity.as_mut() {
        make_room(selection, 0, Some(key));
    }
    cx.notify();
}
