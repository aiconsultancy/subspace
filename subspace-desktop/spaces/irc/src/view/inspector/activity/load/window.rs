//! What a read draws, fitted into its own allowance (SUB decisions
//! `8357ec55` §3, `33ccecf6` item 1 and `4311830a` item 1), never planned
//! beside the snapshot's bodies.

use super::super::Selection;
use super::super::blocks::{Block, page_block, whole_blocks};
use super::super::draw::{Key, MIN_PIECE, Version, cut, piece_size, read_version, replan};
use super::super::ledger::{make_room, room_beside};
use super::super::parts::PART;
use super::super::size::body_bytes;
use super::{Held, Loaded, PAGE, Window};

/// Publishes what the read `key` holds in one step (SUB decision
/// `4311830a` item 6): its window fitted, room made beside it and the plan
/// worked out again, with no request made, so priority stays as the
/// person left it.
pub(super) fn place(selection: &mut Selection, key: &Key) {
    settle(selection, key);
    make_room(selection, 0, Some(key));
    replan(selection);
}

/// Fits what a read draws into what its allowance leaves beside what it
/// holds: a page its drawn window, in parts when that is larger; an item
/// read whole its body drawn block by block when that fits, in parts
/// within what is left otherwise. A whole read's window grows past its
/// allowance only into room nothing holds, no other read and no body
/// asked for, so growing never lets anything go (SUB decision `feb0d839`).
fn settle(selection: &mut Selection, key: &Key) {
    let room = room_beside(selection, key);
    let allowance = regrant(selection, key, room);
    fit_window(selection, key, room.saturating_sub(allowance));
}

/// The steady allowance of the read `key`, within the `room` beside
/// everything else. A read in parts, once its page is in, gets back the
/// room its window gave up while the page came in: a page and a window of
/// what the room leaves beside it, up to a part, never less than it had;
/// it grows only into that room, so growing never lets anything go (SUB
/// decisions `4311830a` item 1 and `a884e4c6`).
fn regrant(selection: &mut Selection, key: &Key, room: u64) -> u64 {
    let Some(loaded) = selection.loaded.get_mut(key) else {
        return 0;
    };
    if matches!(loaded.held, Held::Parts(_)) {
        loaded.allowance = loaded.allowance.max(page_allowance(room).min(room));
    }
    loaded.allowance
}

/// The steady allowance of a read in parts within the `room` beside
/// everything else: a page and a window of what is left beside it, up to
/// a part.
fn page_allowance(room: u64) -> u64 {
    PAGE.saturating_add(room.saturating_sub(PAGE).min(PART as u64))
}

/// Fits every read whose window grew past its allowance back within it:
/// the first thing given back when room is wanted, before anything is let
/// go (SUB decision `feb0d839`).
pub(in super::super) fn give_back(selection: &mut Selection) {
    let grown: Vec<Key> = selection
        .loaded
        .iter()
        .filter(|(_, loaded)| grown(loaded))
        .map(|(key, _)| key.clone())
        .collect();
    for key in grown {
        fit_window(selection, &key, 0);
    }
}

/// Whether what `loaded` holds and draws passes its steady allowance:
/// only an item read whole grows.
fn grown(loaded: &Loaded) -> bool {
    let steady = loaded.raw.saturating_add(loaded.window.charge());
    matches!(loaded.held, Held::Whole(_)) && steady > loaded.allowance
}

/// Grants the read `key` the steady allowance `allowance` and fits its
/// window within it, letting go of any growth past it.
pub(in super::super) fn refit(selection: &mut Selection, key: &Key, allowance: u64) {
    if let Some(loaded) = selection.loaded.get_mut(key) {
        loaded.allowance = allowance;
    }
    fit_window(selection, key, 0);
}

/// Fits the window of the read `key` within its allowance, and for an
/// item read whole the `free` room beyond it.
fn fit_window(selection: &mut Selection, key: &Key, free: u64) {
    let epoch = selection.epoch;
    let Some(loaded) = selection.loaded.get_mut(key) else {
        return;
    };
    let left = loaded.allowance.saturating_sub(loaded.raw);
    let version = read_version(epoch, loaded);
    let old = match std::mem::replace(&mut loaded.window, Window::None) {
        Window::Pieced(old) => Some(old),
        _ => None,
    };
    loaded.window = match &loaded.held {
        Held::Whole(entry) => window(&whole_blocks(entry), (key, version), (left, free), old),
        Held::Parts(parts) => {
            let blocks = [page_block(&parts.page.data)];
            window(&blocks, (key, version), (left, 0), old)
        }
        Held::Loading | Held::Failed(_) => Window::None,
    };
}

/// The window of `blocks`, the body of `key` at `version`, within the
/// `left` bytes of its allowance: block by block when it fits them, or
/// them and the `free` room no read holds; otherwise in parts of which
/// one, its copy and their index fit its allowance, keeping the part `old`
/// showed.
fn window(
    blocks: &[Block<'_>],
    (key, version): (&Key, Version),
    (left, free): (u64, u64),
    old: Option<super::super::draw::Pieced>,
) -> Window {
    let bytes = body_bytes(blocks);
    if bytes <= left.saturating_add(free) {
        return Window::Whole(bytes);
    }
    let size = piece_size(left).unwrap_or(MIN_PIECE);
    Window::Pieced(cut(blocks, (key.clone(), version), (size, bytes), old))
}
