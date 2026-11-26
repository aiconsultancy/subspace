//! Items too large to send inline, read in the background and kept in the
//! selection within one budget ([`MAX_LOADED`], SUB decisions `a17870f4`
//! and `ed71a299`): every byte the view holds of the captain's activity
//! counts against it, the snapshot's inline content and what the open
//! turns draw ([`super::draw`]) too. No item is too large to read: one
//! that fits the room left together with its drawn copy is read whole,
//! page by page; a larger one is read and shown one page of its JSON at a
//! time, never whole. Room is made before each read, the least recently
//! opened item going first, and an item goes with its turn. A read is
//! cancelled when its item goes, so no page is asked for after the view
//! closes, and a reply that arrives after its read was let go or replaced
//! is dropped.
//!
//! Each read holds one reservation from the moment it is made until it is
//! let go or replaced (SUB decisions `8357ec55` §3 and `33ccecf6` item 1):
//! what it holds and the window it draws, together. A whole read reserves
//! the item and its drawn copy; once read, it draws whole within that,
//! or, when its markers and formatting draw more, whole within that and
//! the room no read holds, letting nothing go, else in parts within it. A
//! read in parts reserves a page and its drawn window; while the next part
//! is read, the part shown and its window stay counted beside the page
//! coming in. What a read draws is never planned beside the snapshot's
//! bodies: it always draws, within its own reservation. A new snapshot
//! never lets go of the read the floor keeps: the part of the item last
//! asked for.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{App, AppContext as _, Context, Task};
use subspace_client_fleet::activity::{ActivityEntry, ItemPage, ItemRead};
use subspace_client_fleet::{Error, FleetApi, FleetClient};

use super::Selection;
use super::draw::{Key, Pieced};
use super::parts::PART;
use crate::Irc;
pub(super) use part::show_page;
pub(super) use prune::prune;
use window::{page_window, settle};

mod part;
mod prune;
mod window;

/// The most bytes of the captain's activity one view holds: the
/// snapshot's inline content, the items read for it and what is drawn.
pub(super) const MAX_LOADED: u64 = 64 * 1024 * 1024;
/// The most bytes of item JSON one page carries, and so one part.
const PAGE: u64 = 64 * 1024;
/// What a read in parts reserves: a page, and the window it draws.
const PAGE_READ: u64 = PAGE + PART as u64;
/// The most the read the floor keeps holds: a part and its window, and
/// while the next part is read, the page coming in.
const FLOOR_READ: u64 = PAGE_READ + PAGE;

/// An item read, or being read.
pub(super) struct Loaded {
    pub(super) held: Held,
    /// Its reservation, what it counts against the budget: while it is
    /// read, the room made for it and its window; once read, what it holds
    /// and the window it draws; while another part is read, those and the
    /// page coming in.
    bytes: u64,
    /// The bytes of item JSON held.
    raw: u64,
    /// What it draws within its reservation.
    pub(super) window: Window,
    /// When it was last opened, by [`Selection::opened`].
    pub(super) opened: u64,
    /// Changes whenever what is held does, so its body is measured again
    /// ([`super::draw`]).
    pub(super) version: u64,
    /// The read under way or last made: a reply for any other is dropped.
    read: u64,
    /// An item found larger than its room is read again, once, with room
    /// for the size it claimed, when that fits the budget.
    regrow: bool,
    /// Set when the item goes or its read is replaced, so that read asks
    /// for no further page.
    cancel: Arc<AtomicBool>,
    _read: Option<Task<()>>,
}

impl Drop for Loaded {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

/// What is held of an item.
pub(super) enum Held {
    /// Its first read is under way.
    Loading,
    /// Read in full: of a kind and shape this version knows, or its JSON.
    Whole(ActivityEntry),
    /// Too large to hold whole: one page of its JSON at a time.
    Parts(Parts),
    Failed(Error),
}

/// What a read draws within its reservation.
pub(super) enum Window {
    /// Nothing yet: it is read, or failed.
    None,
    /// Drawn block by block, holding these bytes drawn.
    Whole(u64),
    /// Too large to draw whole within its reservation: one part at a time.
    Pieced(Pieced),
}

impl Window {
    /// The bytes it holds drawn.
    fn charge(&self) -> u64 {
        match self {
            Self::None => 0,
            Self::Whole(bytes) => *bytes,
            Self::Pieced(pieced) => pieced.charge(),
        }
    }
}

/// An item read part by part: the page shown, of the version every part
/// is pinned to.
pub(super) struct Parts {
    pub(super) page: ItemPage,
    /// The item changed while it was read, so it starts again from the
    /// first part of its newest version.
    pub(super) newer: bool,
    /// The part being read, shown once it arrives.
    pub(super) loading: Option<u32>,
    /// The part whose read failed, and why.
    pub(super) failed: Option<(u32, Error)>,
}

/// The bytes the budget leaves for items once the inline content is
/// counted.
pub(super) fn room(selection: &Selection) -> u64 {
    selection.budget.saturating_sub(selection.inline)
}

/// Whether an item of `bytes` is read whole: when it and its drawn copy
/// both fit the room (SUB decision `6f5e67dd`, item 2).
pub(super) fn fits_whole(selection: &Selection, bytes: u64) -> bool {
    bytes.saturating_mul(2) <= room(selection)
}

/// Reads an item that was too large to send inline: whole when its
/// declared size, `bytes`, and its drawn copy fit the room left, part by
/// part otherwise.
pub(super) fn load(board: &mut Irc, turn: &str, item: &str, bytes: u64, cx: &mut Context<Irc>) {
    read_item(board, (turn.to_owned(), item.to_owned()), (bytes, true), cx);
}

/// Reads the item `key` of declared size `bytes` within the room made for
/// it before the read: a whole read never holds more than that room, and
/// page 0 claiming more comes back alone. Once, when `regrow`, an item
/// that grew past its room but fits the budget is read again with room
/// made for the size its page 0 claimed.
fn read_item(
    board: &mut Irc,
    key: (String, String),
    (bytes, regrow): (u64, bool),
    cx: &mut Context<Irc>,
) {
    let client = board
        .inspector_activity
        .as_ref()
        .and_then(|selection| client(selection, cx));
    let Some((selection, client)) = board.inspector_activity.as_mut().zip(client) else {
        return;
    };
    // Any read of the item already in flight goes first.
    selection.loaded.remove(&key);
    let room = room(selection);
    // Room for the whole item and its drawn copy, and at least for its
    // page 0; or for one page and its drawn window.
    let (reserve, drawn) = if fits_whole(selection, bytes) {
        (bytes.max(PAGE.min(room)), bytes)
    } else {
        (PAGE, PART as u64)
    };
    let reservation = reserve.saturating_add(drawn);
    make_room(selection, reservation, None);
    selection.opened += 1;
    let read = selection.opened;
    // The latest request from its admission: the one the floor keeps.
    selection.asked.insert(key.clone(), read);
    let cancel = Arc::new(AtomicBool::new(false));
    let (agent, read_key, flag) = (selection.agent.clone(), key.clone(), cancel.clone());
    let task = cx.spawn(async move |board, cx| {
        let (turn, item) = read_key.clone();
        let result = cx
            .background_spawn(
                async move { client.activity_item(&agent, &turn, &item, reserve, &flag) },
            )
            .await;
        let Some(result) = result else {
            return;
        };
        _ = board.update(cx, |board, cx| {
            publish(board, (&read_key, read), result, cx)
        });
    });
    selection.loaded.insert(
        key,
        Loaded {
            held: Held::Loading,
            bytes: reservation,
            raw: 0,
            window: Window::None,
            opened: read,
            version: read,
            read,
            regrow,
            cancel,
            _read: Some(task),
        },
    );
    super::draw::replan(selection);
    cx.notify();
}

/// The client to read the captain's fleet with.
fn client(selection: &Selection, cx: &App) -> Option<FleetClient> {
    #[cfg(test)]
    if let Some(client) = &selection.client {
        return Some(client.clone());
    }
    let api = FleetApi::current(cx)?;
    api.read(cx).client(&selection.fleet).ok()
}

/// The item read `read` of `key` is still wanted: neither let go nor
/// replaced by another read since.
fn current<'a>(
    board: &'a mut Irc,
    (key, read): (&(String, String), u64),
) -> Option<&'a mut Loaded> {
    board
        .inspector_activity
        .as_mut()?
        .loaded
        .get_mut(key)
        .filter(|loaded| loaded.read == read)
}

/// Shows a read's answer, unless its item went or was read again
/// meanwhile, and keeps what is held within the budget.
fn publish(
    board: &mut Irc,
    (key, read): (&(String, String), u64),
    result: Result<ItemRead, Error>,
    cx: &mut Context<Irc>,
) {
    if let Ok(ItemRead::Larger(first)) = &result
        && grew(board, (key, read), first.bytes)
    {
        read_item(board, key.clone(), (first.bytes, false), cx);
        return;
    }
    let Some(loaded) = current(board, (key, read)) else {
        return;
    };
    (loaded.held, loaded.raw) = match result {
        Ok(ItemRead::Whole { entry, bytes }) => (Held::Whole(entry), bytes),
        Ok(ItemRead::Larger(first)) => {
            let bytes = first.data.len() as u64;
            let parts = Parts {
                page: first,
                newer: false,
                loading: None,
                failed: None,
            };
            (Held::Parts(parts), bytes)
        }
        Ok(_) => (Held::Failed(Error::InvalidResponse), 0),
        Err(error) => (Held::Failed(error), 0),
    };
    if let Some(selection) = board.inspector_activity.as_mut() {
        restamp(selection, key);
        settle(selection, key);
        make_room(selection, 0, Some(key));
        // Read to be seen: its body is asked for, and room made for it.
        super::draw::ask(selection, key);
    }
    cx.notify();
}

/// Marks what is held of `key` as changed, so its body is measured again.
fn restamp(selection: &mut Selection, key: &(String, String)) {
    selection.opened += 1;
    if let Some(loaded) = selection.loaded.get_mut(key) {
        loaded.version = selection.opened;
    }
}

/// The item read `read` of `key`, still wanted, came back larger than
/// the room made for it, may be read again, and fits the budget, with
/// its drawn copy, at the size its page 0 claimed.
fn grew(board: &mut Irc, (key, read): (&(String, String), u64), bytes: u64) -> bool {
    let regrow = current(board, (key, read)).is_some_and(|loaded| loaded.regrow);
    regrow
        && board
            .inspector_activity
            .as_ref()
            .is_some_and(|selection| fits_whole(selection, bytes))
}

/// The read under way or last made of the item `key`.
#[cfg(test)]
pub(super) fn read_of(selection: &Selection, key: &(String, String)) -> Option<u64> {
    selection.loaded.get(key).map(|loaded| loaded.read)
}

/// Delivers `result` as the reply to read `read` of the item `key`, as a
/// read in the background would, however late.
#[cfg(test)]
pub(super) fn reply(
    board: &mut Irc,
    (key, read): (&(String, String), u64),
    result: Result<ItemRead, Error>,
    cx: &mut Context<Irc>,
) {
    publish(board, (key, read), result, cx);
}

/// The bytes the items read hold, or were made room for while read.
pub(super) fn held(selection: &Selection) -> u64 {
    selection.loaded.values().map(|loaded| loaded.bytes).sum()
}

/// Lets go of the least recently opened items, other than `keep`, until
/// `bytes` more fit in the budget beside the inline content. An item let
/// go shows its load action again; a read under way is cancelled and its
/// reply dropped. What is drawn is fitted afterwards ([`super::draw`]).
fn make_room(selection: &mut Selection, bytes: u64, keep: Option<&Key>) {
    loop {
        if held(selection).saturating_add(bytes) <= room(selection) {
            return;
        }
        let Some(oldest) = selection
            .loaded
            .iter()
            .filter(|(key, _)| Some(*key) != keep)
            .min_by_key(|(_, loaded)| loaded.opened)
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        selection.loaded.remove(&oldest);
    }
}
