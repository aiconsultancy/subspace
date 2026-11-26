//! Items too large to send inline, read in the background and kept in the
//! selection within one budget ([`MAX_LOADED`], SUB decisions `a17870f4`
//! and `ed71a299`): every byte the view holds of the captain's activity
//! counts against it, the snapshot's inline content and what the open turns
//! draw ([`super::draw`]) too. No item is too large to read: one that fits
//! the room left together with its drawn copy is read whole, page by page;
//! a larger one is read and shown one page of its JSON at a time, never
//! whole. Room is made before each read ([`super::ledger`]): growth past an
//! allowance is given back first, then the lowest priority goes, items read
//! and bodies asked for alike; an item goes with its turn. A read is
//! cancelled when its item goes, so no page is asked for after the view
//! closes, and a reply that arrives after its read was let go or replaced
//! is dropped.
//!
//! Each read counts in the one ledger from the moment it is made until it
//! is let go or replaced (SUB decisions `8357ec55` §3, `33ccecf6` item 1
//! and `4311830a`): what it holds and the window it draws, within its
//! steady allowance, and beside it what is coming in. A whole read is
//! granted the item and its drawn copy; once read, it draws whole within
//! that, or, when its markers and formatting draw more, whole within that
//! and room nothing else holds, no other read and no body asked for,
//! letting nothing go, else in parts within it. That growth gives way to
//! any newer request, given back before anything is let go (SUB decision
//! `feb0d839`). A read in parts is granted a page and its drawn window;
//! while the next part is read, the part shown and its window stay counted
//! beside the page coming in, and once it arrives that page and its window
//! are counted in their place. **Load the latest** keeps what was read
//! shown and counted until the newer version arrives, or beside its error.
//! What a read draws is never planned beside the snapshot's bodies: it
//! always draws, within its own allowance. A new snapshot never lets go of
//! the read the floor keeps: the part of the item last asked for.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{App, AppContext as _, Context, Task};
use subspace_client_fleet::activity::{ActivityEntry, ItemPage, ItemRead};
use subspace_client_fleet::{Error, FleetApi, FleetClient};

use super::Selection;
use super::draw::{Key, Pieced};
use super::ledger::{make_room, room, spare, stamp, window_allowance};
use super::parts::PART;
use crate::Irc;
pub(super) use part::show_page;
pub(super) use prune::prune;
use window::place;
pub(super) use window::{give_back, refit};

mod part;
mod prune;
mod window;

/// The most bytes of the captain's activity one view holds: the
/// snapshot's inline content, the items read for it and what is drawn.
pub(super) const MAX_LOADED: u64 = 64 * 1024 * 1024;
/// The most bytes of item JSON one page carries, and so one part.
pub(super) const PAGE: u64 = 64 * 1024;
/// The floor's steady allowance for a read (SUB decision `4311830a` item
/// 1): a part and its drawn window, beside at most a page coming in.
pub(super) const PAGE_READ: u64 = PAGE + PART as u64;

/// An item read, or being read.
pub(super) struct Loaded {
    pub(super) held: Held,
    /// Its steady allowance ([`super::ledger`]): what it may hold and draw
    /// once read, granted at admission or sized again when the room
    /// shrinks.
    allowance: u64,
    /// The temporary allowance for what is coming in: while a part is read,
    /// the page coming in.
    incoming: u64,
    /// The bytes of item JSON held.
    raw: u64,
    /// What it draws within its allowance.
    pub(super) window: Window,
    /// A newer read replacing what it shows, which stays shown and charged
    /// until the newer one arrives.
    pub(super) reload: Reload,
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

impl Loaded {
    /// A first read under way, granted `allowance`.
    fn reading(
        allowance: u64,
        (read, regrow): (u64, bool),
        (cancel, task): (Arc<AtomicBool>, Task<()>),
    ) -> Self {
        Self {
            held: Held::Loading,
            allowance,
            incoming: 0,
            raw: 0,
            window: Window::None,
            reload: Reload::None,
            version: read,
            read,
            regrow,
            cancel,
            _read: Some(task),
        }
    }

    /// What it counts against the budget: its steady charge, its allowance
    /// while its first read is under way, and what comes in for it.
    pub(super) fn charge(&self) -> u64 {
        let steady = match self.held {
            Held::Loading => self.allowance,
            _ => self.raw.saturating_add(self.window.charge()),
        };
        steady.saturating_add(self.incoming)
    }

    /// Whether it shows what was read: kept, shown and charged while a
    /// newer read replaces it.
    fn shows(&self) -> bool {
        matches!(self.held, Held::Whole(_) | Held::Parts(_))
    }

    /// Replaces what it shows with read `read`, granted `allowance`, which
    /// counts as coming in until it arrives; what it shows stays shown and
    /// charged meanwhile.
    fn reload(
        &mut self,
        allowance: u64,
        (read, regrow): (u64, bool),
        (cancel, task): (Arc<AtomicBool>, Task<()>),
    ) {
        self.cancel.store(true, Ordering::Release);
        self.incoming = allowance;
        self.reload = Reload::Reading(allowance);
        self.read = read;
        self.regrow = regrow;
        self.cancel = cancel;
        self._read = Some(task);
    }

    /// What the ledger fits it by: what it holds, what comes in for it,
    /// and, while its first read is under way, its allowance.
    pub(super) fn charges(&self) -> (u64, u64, Option<u64>) {
        let reading = matches!(self.held, Held::Loading).then_some(self.allowance);
        (self.raw, self.incoming, reading)
    }
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

/// A newer read of an item, replacing what it shows (SUB decision
/// `4311830a` item 2).
pub(super) enum Reload {
    None,
    /// Under way, granted this allowance once it arrives.
    Reading(u64),
    /// It failed: what was shown stays, beside the error and a retry.
    Failed(Error),
}

/// What a read draws within its allowance.
pub(super) enum Window {
    /// Nothing yet: it is read, or failed.
    None,
    /// Drawn block by block, holding these bytes drawn.
    Whole(u64),
    /// Too large to draw whole within its allowance: one part at a time.
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
/// made for the size its page 0 claimed; only the first read is the
/// person's request, so only it changes priority ([`stamp`]). What an
/// earlier read shows stays shown and charged until this one arrives
/// (SUB decision `4311830a` item 2).
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
    // A first read still in flight, or failed, goes first; what was read
    // stays until the new read arrives.
    if !selection.loaded.get(&key).is_some_and(Loaded::shows) {
        selection.loaded.remove(&key);
    }
    if regrow {
        stamp(selection, &key);
    }
    let read = next_read(selection);
    let (reserve, allowance) = admit(selection, &key, bytes);
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
    match selection.loaded.get_mut(&key) {
        Some(shown) if shown.shows() => shown.reload(allowance, (read, regrow), (cancel, task)),
        _ => {
            let loaded = Loaded::reading(allowance, (read, regrow), (cancel, task));
            selection.loaded.insert(key, loaded);
        }
    }
    super::draw::replan(selection);
    cx.notify();
}

/// Makes room for a read of `key` of declared size `bytes`, and what it
/// reserves for the read and is granted in all: room for the whole item
/// and its drawn copy, and at least for its page 0, when both fit;
/// otherwise a page, and a window sized to the room left beside it, or
/// the floor's when none is.
fn admit(selection: &mut Selection, key: &Key, bytes: u64) -> (u64, u64) {
    if fits_whole(selection, bytes) {
        let reserve = whole_reserve(room(selection), bytes);
        let allowance = reserve.saturating_add(bytes);
        make_room(selection, allowance, Some(key));
        return (reserve, allowance);
    }
    make_room(selection, PAGE_READ, Some(key));
    let window = window_allowance(spare(selection, key).saturating_sub(PAGE));
    (PAGE, PAGE.saturating_add(window))
}

/// What a whole read of `bytes` reserves for the item within `room`: the
/// item, and at least its page 0, beside its drawn copy.
fn whole_reserve(room: u64, bytes: u64) -> u64 {
    bytes.max(PAGE.min(room.saturating_sub(bytes)))
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
    match (
        outcome(result),
        std::mem::replace(&mut loaded.reload, Reload::None),
    ) {
        // What was shown stays, beside the error and a retry.
        (Err(error), Reload::Reading(_)) => loaded.reload = Reload::Failed(error),
        (Err(error), _) => (loaded.held, loaded.raw) = (Held::Failed(error), 0),
        (Ok(read), reload) => {
            (loaded.held, loaded.raw) = read;
            if let Reload::Reading(allowance) = reload {
                loaded.allowance = allowance;
            }
        }
    }
    loaded.incoming = 0;
    if let Some(selection) = board.inspector_activity.as_mut() {
        restamp(selection, key);
        place(selection, key);
    }
    cx.notify();
}

/// What a read's answer holds, and the bytes of item JSON in it.
fn outcome(result: Result<ItemRead, Error>) -> Result<(Held, u64), Error> {
    match result? {
        ItemRead::Whole { entry, bytes } => Ok((Held::Whole(entry), bytes)),
        ItemRead::Larger(first) => {
            let bytes = first.data.len() as u64;
            let parts = Parts {
                page: first,
                newer: false,
                loading: None,
                failed: None,
            };
            Ok((Held::Parts(parts), bytes))
        }
        _ => Err(Error::InvalidResponse),
    }
}

/// The name of a new read, or of a change to what an item read holds:
/// never a request, so priority stays.
fn next_read(selection: &mut Selection) -> u64 {
    selection.reads += 1;
    selection.reads
}

/// Marks what is held of `key` as changed, so its body is measured again.
fn restamp(selection: &mut Selection, key: &(String, String)) {
    let version = next_read(selection);
    if let Some(loaded) = selection.loaded.get_mut(key) {
        loaded.version = version;
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
