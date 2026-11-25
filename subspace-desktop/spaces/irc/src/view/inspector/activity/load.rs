//! Items too large to send inline, read in the background and kept in the
//! selection within one budget ([`MAX_LOADED`], SUB decision `a17870f4`):
//! every byte the view holds of the captain's activity counts against it,
//! the snapshot's inline content too. No item is too large to read: one
//! that fits the room left is read whole, page by page; a larger one is
//! read and shown one page of its JSON at a time, never whole. Room is
//! made before each read, the least recently opened item going first, and
//! an item goes with its turn. A read is cancelled when its item goes, so
//! no page is asked for after the view closes, and a reply that arrives
//! after its read was let go or replaced is dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::{App, AppContext as _, Context, Task};
use serde_json::Value;
use subspace_client_fleet::activity::{
    ActivityContent, ActivityEntry, ActivityError, ActivityItem, AgentActivity, ItemPage, ItemPart,
    ItemRead,
};
use subspace_client_fleet::{Error, FleetApi, FleetClient};

use super::Selection;
use crate::Irc;

/// The most bytes of the captain's activity one view holds: the
/// snapshot's inline content and the items read for it.
pub(super) const MAX_LOADED: u64 = 64 * 1024 * 1024;
/// The most bytes of item JSON one page carries, and so one part.
const PAGE: u64 = 64 * 1024;

/// An item read, or being read.
pub(super) struct Loaded {
    pub(super) held: Held,
    /// What it counts against the budget: the bytes held, or while it is
    /// read, the bytes room was made for.
    bytes: u64,
    /// When it was last opened, by [`Selection::opened`].
    opened: u64,
    /// The read under way or last made: a reply for any other is dropped.
    read: u64,
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

/// Reads an item that was too large to send inline: whole when its
/// declared size, `bytes`, fits the room left, part by part otherwise.
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
    let reserve = if bytes <= room(selection) {
        bytes
    } else {
        PAGE
    };
    make_room(selection, reserve, None);
    // An item that grew past what was made room for is read in parts.
    let limit = room(selection).saturating_sub(held(selection));
    selection.opened += 1;
    let read = selection.opened;
    let cancel = Arc::new(AtomicBool::new(false));
    let (agent, read_key, flag) = (selection.agent.clone(), key.clone(), cancel.clone());
    let task = cx.spawn(async move |board, cx| {
        let (turn, item) = read_key.clone();
        let result = cx
            .background_spawn(
                async move { client.activity_item(&agent, &turn, &item, limit, &flag) },
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
            bytes: reserve,
            opened: read,
            read,
            cancel,
            _read: Some(task),
        },
    );
    cx.notify();
}

/// Shows part `page` of an item read part by part, pinned to the version
/// of the part shown.
pub(super) fn show_page(board: &mut Irc, key: &(String, String), page: u32, cx: &mut Context<Irc>) {
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
    let Some(loaded) = current(board, (key, read)) else {
        return;
    };
    (loaded.held, loaded.bytes) = match result {
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
        make_room(selection, 0, Some(key));
    }
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
/// reply dropped.
fn make_room(selection: &mut Selection, bytes: u64, keep: Option<&(String, String)>) {
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

/// Counts the snapshot's inline content, lets go of the items whose turn
/// is no longer in it, and of the parts shown of their texts, and keeps
/// the rest within the budget.
pub(super) fn prune(selection: &mut Selection) {
    let Some(activity) = selection.join.activity() else {
        return;
    };
    let inline = inline_bytes(activity);
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
    selection.inline = inline;
    make_room(selection, 0, None);
}

/// About how many bytes the snapshot's inline content holds: the length
/// of every text, input, output and value in it.
pub(super) fn inline_bytes(activity: &AgentActivity) -> u64 {
    activity
        .turns
        .iter()
        .flat_map(|turn| &turn.items)
        .map(entry_bytes)
        .sum()
}

fn entry_bytes(entry: &ActivityEntry) -> u64 {
    let bytes = match entry {
        // Its identity only: its content is read on demand.
        ActivityEntry::Paged(_) => 0,
        ActivityEntry::Item(item) => item_bytes(item),
        ActivityEntry::Unknown(unknown) => value_bytes(&unknown.raw),
    };
    bytes.saturating_add(entry.base().id.len() as u64)
}

fn item_bytes(item: &ActivityItem) -> u64 {
    let text = |text: &str| text.len() as u64;
    let error = |error: Option<&ActivityError>| {
        error.map_or(0, |error| {
            text(&error.message) + error.response_body.as_deref().map_or(0, text)
        })
    };
    match item {
        ActivityItem::Text(words) | ActivityItem::Reasoning(words) => text(&words.text),
        ActivityItem::Step(step) => error(step.error.as_ref()),
        ActivityItem::Tool(tool) => {
            let content: u64 = tool
                .content
                .iter()
                .flatten()
                .map(|content| match content {
                    ActivityContent::Text { text: words } => text(words),
                    ActivityContent::File { uri, .. } => text(uri),
                    ActivityContent::Other(raw) => value_bytes(raw),
                })
                .sum();
            text(&tool.raw_input)
                + tool.input.as_ref().map_or(0, value_bytes)
                + tool.metadata.as_ref().map_or(0, value_bytes)
                + content
                + error(tool.error.as_ref())
        }
    }
}

/// About how many bytes a JSON value holds: its strings and keys, and
/// eight for any other value.
fn value_bytes(value: &Value) -> u64 {
    match value {
        Value::String(text) => text.len() as u64,
        Value::Array(values) => values.iter().map(value_bytes).sum(),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| key.len() as u64 + value_bytes(value))
            .sum(),
        _ => 8,
    }
}
