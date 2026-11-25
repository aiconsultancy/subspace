//! An item too large to send inline, read page by page in the
//! background and kept whole in the selection: at most [`MAX_LOADED`]
//! bytes of them, the least recently opened going first, and none whose
//! turn left the snapshot. A read is cancelled when its item goes, so no
//! page is asked for after the view closes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{App, AppContext as _, Context, Task};
use subspace_client_fleet::activity::ActivityItem;
use subspace_client_fleet::{Error, FleetApi, FleetClient};

use super::Selection;
use crate::Irc;

/// The most bytes of items read in full that one view keeps.
const MAX_LOADED: u64 = 64 * 1024 * 1024;

/// An item read in full, or being read.
pub(super) struct Loaded {
    pub(super) result: Option<Result<ActivityItem, Error>>,
    /// The item's size as its page 0 declares it.
    bytes: u64,
    /// When it was last opened, by [`Selection::opened`].
    opened: u64,
    /// Set when the item goes, so its read asks for no further page.
    cancel: Arc<AtomicBool>,
    _read: Option<Task<()>>,
}

impl Drop for Loaded {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

/// Reads an item that was too large to send inline, page by page. `bytes`
/// is its declared size.
pub(super) fn load(board: &mut Irc, turn: &str, item: &str, bytes: u64, cx: &mut Context<Irc>) {
    let client = board
        .inspector_activity
        .as_ref()
        .and_then(|selection| client(selection, cx));
    let Some((selection, client)) = board.inspector_activity.as_mut().zip(client) else {
        return;
    };
    let key = (turn.to_owned(), item.to_owned());
    // Any read of the item already in flight goes first.
    selection.loaded.remove(&key);
    make_room(selection, bytes);
    let cancel = Arc::new(AtomicBool::new(false));
    let (agent, read_key, flag) = (selection.agent.clone(), key.clone(), cancel.clone());
    let read = cx.spawn(async move |board, cx| {
        let (turn, item) = read_key.clone();
        let result = cx
            .background_spawn(async move { client.activity_item(&agent, &turn, &item, &flag) })
            .await;
        let Some(result) = result else {
            return;
        };
        _ = board.update(cx, |board, cx| publish(board, &read_key, result, cx));
    });
    selection.opened += 1;
    selection.loaded.insert(
        key,
        Loaded {
            result: None,
            bytes,
            opened: selection.opened,
            cancel,
            _read: Some(read),
        },
    );
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

/// Shows a read's answer, unless its item went meanwhile.
fn publish(
    board: &mut Irc,
    key: &(String, String),
    result: Result<ActivityItem, Error>,
    cx: &mut Context<Irc>,
) {
    if let Some(loaded) = board
        .inspector_activity
        .as_mut()
        .and_then(|selection| selection.loaded.get_mut(key))
    {
        loaded.result = Some(result);
        cx.notify();
    }
}

/// Lets go of the least recently opened items until `bytes` more fit in
/// [`MAX_LOADED`]. An item let go shows **Load in full** again.
fn make_room(selection: &mut Selection, bytes: u64) {
    loop {
        let held: u64 = selection.loaded.values().map(|loaded| loaded.bytes).sum();
        if held.saturating_add(bytes) <= MAX_LOADED {
            return;
        }
        let Some(oldest) = selection
            .loaded
            .iter()
            .min_by_key(|(_, loaded)| loaded.opened)
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        selection.loaded.remove(&oldest);
    }
}

/// Lets go of the items whose turn is no longer in the snapshot, and of
/// the parts shown of their texts.
pub(super) fn prune(selection: &mut Selection) {
    let Some(activity) = selection.join.activity() else {
        return;
    };
    let turns: std::collections::HashSet<&str> = activity
        .turns
        .iter()
        .map(|turn| turn.summary.id.as_str())
        .collect();
    selection
        .loaded
        .retain(|(turn, _), _| turns.contains(turn.as_str()));
    selection.parts.retain(|block, _| {
        block
            .strip_prefix("irc-activity:")
            .and_then(|rest| rest.split(':').next())
            .is_none_or(|turn| turns.contains(turn))
    });
}
