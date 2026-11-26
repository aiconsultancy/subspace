//! What a read draws, fitted into its own reservation (SUB decisions
//! `8357ec55` §3 and `33ccecf6` item 1), never planned beside the
//! snapshot's bodies.

use super::super::Selection;
use super::super::blocks::{Block, page_block, whole_blocks};
use super::super::draw::{Key, MIN_PIECE, Version, cut, read_version};
use super::super::parts::PART;
use super::super::size::body_bytes;
use super::{Held, Window, held, room};

/// Fits what a read draws into what its reservation leaves beside what it
/// holds, and counts the two: a page its drawn window, at most a part; an
/// item read whole its body drawn block by block when that fits, in parts
/// within what is left otherwise. Its reservation grows only into room
/// no read holds, never letting anything go.
pub(super) fn settle(selection: &mut Selection, key: &Key) {
    let epoch = selection.epoch;
    // The room no read holds: a whole read's formatting may draw into it,
    // letting nothing go.
    let free = room(selection).saturating_sub(held(selection));
    let Some(loaded) = selection.loaded.get_mut(key) else {
        return;
    };
    let left = loaded.bytes.saturating_sub(loaded.raw);
    let window = match &loaded.held {
        Held::Whole(entry) => {
            let version = read_version(epoch, loaded);
            whole_window(&whole_blocks(entry), (key, version), (left, free))
        }
        Held::Parts(parts) => Window::Whole(page_window(&parts.page.data)),
        Held::Loading | Held::Failed(_) => Window::None,
    };
    loaded.window = window;
    loaded.bytes = loaded.raw.saturating_add(loaded.window.charge());
}

/// The window of an item read whole, of the version `version`, within
/// the `left` bytes of its reservation: block by block when its body fits
/// them, or them and the `free` room no read holds; otherwise in parts of
/// which one and its copy fit its reservation.
fn whole_window(
    blocks: &[Block<'_>],
    (key, version): (&Key, Version),
    (left, free): (u64, u64),
) -> Window {
    let bytes = body_bytes(blocks);
    if bytes <= left.saturating_add(free) {
        return Window::Whole(bytes);
    }
    let size = usize::try_from(left / 2)
        .unwrap_or(usize::MAX)
        .clamp(MIN_PIECE, PART);
    Window::Pieced(cut(blocks, (key.clone(), version), (size, bytes), None))
}

/// What one page of JSON draws: at most one part.
pub(super) fn page_window(data: &str) -> u64 {
    body_bytes(&[page_block(data)])
}
