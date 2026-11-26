//! What a read draws, fitted into its own allowance (SUB decisions
//! `8357ec55` §3, `33ccecf6` item 1 and `4311830a` item 1), never planned
//! beside the snapshot's bodies.

use super::super::Selection;
use super::super::blocks::{Block, page_block, whole_blocks};
use super::super::draw::{Key, MIN_PIECE, Version, cut, piece_size, read_version};
use super::super::ledger::{held, left, room};
use super::super::parts::PART;
use super::super::size::body_bytes;
use super::{Held, PAGE, Window};

/// Fits what a read draws into what its allowance leaves beside what it
/// holds: a page its drawn window, in parts when that is larger; an item
/// read whole its body drawn block by block when that fits, in parts
/// within what is left otherwise. A whole read's window grows past its
/// allowance only into room no read holds, never letting anything go.
pub(super) fn settle(selection: &mut Selection, key: &Key) {
    regrant(selection, key);
    let free = room(selection).saturating_sub(granted(selection, key));
    fit_window(selection, key, free);
}

/// Gives a read in parts, once its page is in, the room its window gave
/// up while the page came in: a page and a window of what the room leaves
/// beside everything else, up to a part, never less than it had (SUB
/// decision `4311830a` item 1).
fn regrant(selection: &mut Selection, key: &Key) {
    let room = left(selection, key);
    if let Some(loaded) = selection.loaded.get_mut(key)
        && matches!(loaded.held, Held::Parts(_))
    {
        loaded.allowance = loaded.allowance.max(page_allowance(room));
    }
}

/// The steady allowance of a read in parts within the `room` beside
/// everything else: a page and a window of what is left beside it, up to
/// a part.
fn page_allowance(room: u64) -> u64 {
    PAGE.saturating_add(room.saturating_sub(PAGE).min(PART as u64))
}

/// What the reads count, `key`'s at least its whole allowance.
fn granted(selection: &Selection, key: &Key) -> u64 {
    let (own, allowance) = selection
        .loaded
        .get(key)
        .map_or((0, 0), |loaded| (loaded.charge(), loaded.allowance));
    held(selection)
        .saturating_sub(own)
        .saturating_add(own.max(allowance))
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
