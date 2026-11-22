//! Reads the conversation's messages it has not read yet for tile owners,
//! in transcript order, before their rows are laid out (`crate::tiles`).

use std::rc::Rc;

use gpui_kit::App;

use super::Conversation;
use crate::lines::Item;
use crate::state::Buffer;
use crate::tiles::{TileRef, Tiles};
use crate::transcript::Row;

/// The conversation's tiles once every message `rows` shows has been read.
/// A conversation whose buffer the hub no longer holds reads nothing, so
/// its owners are not let go as if every message had left.
pub(super) fn follow(conversation: &Conversation, rows: &[Row], cx: &mut App) -> Rc<Tiles> {
    let buffer = conversation.buffer.clone();
    let Some((recognition, held, found)) = found(conversation, rows, cx) else {
        return conversation.hub.read(cx).tiles.get(&buffer);
    };
    conversation.hub.update(cx, |hub, _| {
        hub.tiles.read(&buffer, recognition, (&held, &found))
    })
}

/// The issue recognition now, every message shown, and what the messages
/// not read yet name, in transcript order; `None` while the hub holds no
/// buffer for the conversation.
fn found(
    conversation: &Conversation,
    rows: &[Row],
    cx: &App,
) -> Option<(u64, Vec<i64>, Vec<(i64, Vec<TileRef>)>)> {
    let mentions = super::mentions(conversation, cx);
    let hub = conversation.hub.read(cx);
    let recognition = hub.issues.recognition_revision();
    let held = hub.state.buffer(&conversation.buffer)?;
    let messages = messages(rows, held);
    let seqs: Vec<i64> = messages.iter().map(|(_, seq)| *seq).collect();
    let unread = hub
        .tiles
        .get(&conversation.buffer)
        .unread(&seqs, recognition);
    let found = messages
        .iter()
        .filter(|(_, seq)| unread.contains(seq))
        .map(|(at, seq)| {
            let refs = crate::transcript::tile_refs(&held.entries[*at], Some(hub), &mentions);
            (*seq, refs.iter().filter_map(TileRef::of).collect())
        })
        .collect();
    Some((recognition, seqs, found))
}

/// The messages `rows` show, in transcript order, by index and sequence.
fn messages(rows: &[Row], held: &Buffer) -> Vec<(usize, i64)> {
    rows.iter()
        .filter_map(|row| match row {
            Row::Item(Item::Message { at, .. }) => {
                held.entries.get(*at).map(|entry| (*at, entry.seq))
            }
            _ => None,
        })
        .collect()
}
