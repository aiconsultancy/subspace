//! Which item bodies the view draws (SUB decisions `a17870f4` §5,
//! `ed71a299`, `0c13b784`, `8357ec55` and `33ccecf6`). What the open turns
//! draw of the snapshot counts against the one budget beside the inline
//! content and the items read, in the UTF-8 bytes it mounts: each block as
//! it draws, markers and all, or one part when it is long, plus the
//! formatted text kept for it (an unknown item's indented JSON, an error).
//! An item read on demand draws within its own reservation
//! ([`super::load`]) and is never planned here: only what its placeholder
//! names is. Collapsed turns build no bodies, so they draw nothing.
//!
//! Each body is measured once per version of its item: its snapshot's
//! generation, its `seq`, and for an item read on demand what is held of
//! it. The plan below is worked out again only when the snapshot, the
//! items read, the turns opened or the bodies asked for change: drawing a
//! frame neither measures nor formats anything already drawn.
//!
//! The order: the bodies the person asked for come first, the latest
//! first; then the open turns, those opened by hand first, the most
//! recently opened first, then those open by default because they are
//! running, the newest first; and their items in order. Each is drawn
//! while it fits the room the budget leaves beside the inline content and
//! the items read; one that would pass it shows its heading, a caption
//! with its size saying it is not drawn to stay within the view's memory
//! budget, and **Show**. Nothing is cut: every body is one click away.
//! **Show** first makes room, letting go of the items read and the bodies
//! asked for earlier, the least recently asked for first
//! ([`super::ledger`]), until what is held and drawn fits beside the body
//! asked for. If
//! the body still does not fit, it is drawn in parts within the room left,
//! of at most 64 KiB each, with First, Previous, Next and Last
//! ([`super::pieces`]): never hidden and never cut, and only the part
//! shown is written out.
//!
//! The floor (SUB decisions `8357ec55` §2 and `4311830a` item 1): inline
//! content is the snapshot itself and is never let go. Only the latest
//! request may pass the room, and only when no room is left (less than one
//! character and its copy); when it is an item read, its read keeps the
//! floor, never an earlier body. A body that is the latest request is then
//! drawn whole up to one part, or one part of at most 64 KiB at a time,
//! the part and its drawn copy held beside the inline content. With any
//! room left, every body asked for is drawn within it.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::rc::Rc;

use subspace_client_fleet::activity::{ActivityEntry, ActivityJoin, ActivityTurn};

use super::blocks::{Block, entry_blocks, page_block, whole_blocks};
use super::ledger::{held, latest, make_room, room, stamp};
use super::load::Window;
use super::parts::{PART, Prepare};
use super::pieces::{INDEX_BYTES, Index, Piece, index, piece};
use super::size::body_bytes;
use super::turn::is_open;
use super::{Held, Loaded, Selection};

mod cards;

pub(super) use cards::{held_back, held_back_parts, pieced, pieced_parts};

/// An item, by its turn and item IDs.
pub(super) type Key = (String, String);

/// One version of an item's body: the generation of the snapshot it came
/// in ([`Selection::epoch`]), its `seq`, and for an item read on demand
/// the version of what is held of it ([`Loaded::version`]). A `seq` is
/// only ever compared within one generation.
pub(super) type Version = (u64, u64, u64);

/// The fewest bytes a part draws: a marker, the most one character draws.
pub(super) const MIN_PIECE: usize = 12;

/// The fewest bytes a body drawn in parts holds: the smallest part, its
/// copy and the index of where its parts start. A room smaller is no room
/// at all.
pub(super) const MIN_WINDOW: u64 = (2 * MIN_PIECE + INDEX_BYTES) as u64;

/// The most bytes a part draws: one, its copy and the index of where the
/// parts start fit two parts' bytes ([`PART`]).
pub(crate) const PIECE: usize = PART - INDEX_BYTES / 2;

/// An item's body as last measured.
pub(super) struct Measure {
    version: Version,
    pub(super) bytes: u64,
}

/// What the open turns draw, worked out when it changes ([`replan`]).
#[derive(Default)]
pub(super) struct Plan {
    /// The items not drawn, with the bytes their bodies would hold.
    pub(super) held_back: HashMap<Key, u64>,
    /// The body drawn in parts, if any: the one last asked for.
    pub(super) pieced: Option<Pieced>,
    /// The bytes the bodies drawn hold, for a test to check against the
    /// room; drawing reads only what is held back or drawn in parts.
    #[cfg(test)]
    pub(super) drawn: u64,
}

/// A body too large for its room, drawn one part at a time: where each of
/// its parts starts, found once for its version, and the part shown.
pub(super) struct Pieced {
    pub(super) key: Key,
    version: Version,
    index: Index,
    /// The bytes it would hold drawn whole.
    pub(super) bytes: u64,
    pub(super) piece: Piece,
}

impl Pieced {
    /// What it holds: the part kept, its drawn copy and the index of where
    /// the parts start (SUB decision `4311830a` item 5).
    pub(super) fn charge(&self) -> u64 {
        2 * self.index.size as u64 + self.index.bytes()
    }

    /// Shows part `wanted` of `blocks`, the body it was cut from: that
    /// part alone is written out.
    fn show(&mut self, blocks: &[Block<'_>], wanted: usize) {
        self.piece = piece(blocks, &self.index, wanted);
    }
}

/// `blocks`, the body of `key` at `version`, drawn in parts of `size`
/// bytes: `old` again when it is the same body cut the same way, so
/// nothing is scanned twice; otherwise cut once, showing the part `old`
/// showed of it, or the first.
pub(super) fn cut(
    blocks: &[Block<'_>],
    (key, version): (Key, Version),
    (size, bytes): (usize, u64),
    old: Option<Pieced>,
) -> Pieced {
    let old = match old.filter(|old| old.key == key) {
        Some(old) if old.version == version && old.index.size == size => {
            return Pieced { bytes, ..old };
        }
        old => old,
    };
    let wanted = old.map_or(0, |old| old.piece.piece);
    let index = index(blocks, size);
    let piece = piece(blocks, &index, wanted);
    Pieced {
        key,
        version,
        index,
        bytes,
        piece,
    }
}

/// Measures what changed and works out again which bodies the open turns
/// draw within the room left. Run after every change to what they draw,
/// never while drawing.
pub(super) fn replan(selection: &mut Selection) {
    measure(selection);
    let bodies = bodies(selection);
    let room = room(selection).saturating_sub(held(selection));
    let fit = fit(&bodies, (&selection.asked, latest(selection)), room);
    let old = selection.plan.pieced.take();
    let pieced = fit
        .pieced
        .and_then(|wanted| cut_piece(selection, wanted, old));
    selection.plan = Plan {
        held_back: fit.held_back,
        pieced,
        #[cfg(test)]
        drawn: fit.drawn,
    };
    forget_prepared(selection);
}

/// The version of the body of `entry` in the snapshot of `epoch`.
fn version(epoch: u64, entry: &ActivityEntry, loaded: Option<&Loaded>) -> Version {
    (
        epoch,
        entry.base().seq,
        loaded.map_or(0, |loaded| loaded.version),
    )
}

/// The version of the body of an item read whole, in the snapshot of
/// `epoch`.
pub(super) fn read_version(epoch: u64, loaded: &Loaded) -> Version {
    (epoch, 0, loaded.version)
}

/// Measures each body of the open turns whose version changed since it
/// was last measured, and forgets those no longer drawn.
pub(super) fn measure(selection: &mut Selection) {
    selection.measures = measures(selection);
}

/// Each body of the open turns as measured: again only when its version
/// changed. An item too large to send inline that is read whole draws
/// within its read's own allowance, so it alone is not measured here (SUB
/// decision `4311830a` item 3); any other body is planned.
fn measures(selection: &Selection) -> HashMap<Key, Measure> {
    open_entries(selection)
        .into_iter()
        .filter_map(|(key, entry)| {
            let loaded = selection.loaded.get(&key);
            if read_draws(entry, loaded) {
                return None;
            }
            let version = version(selection.epoch, entry, loaded);
            let bytes = match selection.measures.get(&key) {
                Some(measure) if measure.version == version => measure.bytes,
                _ => body_bytes(&entry_blocks(entry, loaded.map(|loaded| &loaded.held))),
            };
            Some((key, Measure { version, bytes }))
        })
        .collect()
}

/// Whether the read `loaded` draws the body of `entry` itself: when the
/// snapshot names it as too large to send inline and it is read whole.
fn read_draws(entry: &ActivityEntry, loaded: Option<&Loaded>) -> bool {
    matches!(entry, ActivityEntry::Paged(_))
        && loaded.is_some_and(|loaded| matches!(loaded.held, Held::Whole(_)))
}

/// The items of the open turns, in the order they draw, and their items
/// in order.
fn open_entries(selection: &Selection) -> Vec<(Key, &ActivityEntry)> {
    let Some(activity) = selection.join.activity() else {
        return Vec::new();
    };
    open_turns(selection, &activity.turns)
        .into_iter()
        .flat_map(|turn| {
            turn.items
                .iter()
                .map(|entry| ((turn.summary.id.clone(), entry.base().id.clone()), entry))
        })
        .collect()
}

/// The open turns in the order they draw: those opened by hand first, the
/// most recently opened first; then those open by default, the newest
/// first.
fn open_turns<'a>(selection: &Selection, turns: &'a [ActivityTurn]) -> Vec<&'a ActivityTurn> {
    let mut open: Vec<&ActivityTurn> = turns
        .iter()
        .rev()
        .filter(|turn| is_open(selection, &turn.summary))
        .collect();
    // Stable: turns open by default keep their order, newest first, after
    // those opened by hand (`None` sorts last once reversed).
    open.sort_by_key(|turn| Reverse(selection.turn_opened.get(&turn.summary.id).copied()));
    open
}

/// Every body the open turns would draw, as measured, in their order.
fn bodies(selection: &Selection) -> Vec<(Key, u64)> {
    open_entries(selection)
        .into_iter()
        .filter_map(|(key, _)| {
            let bytes = selection.measures.get(&key)?.bytes;
            Some((key, bytes))
        })
        .collect()
}

/// Which bodies a plan draws.
#[derive(Default)]
struct Fit {
    held_back: HashMap<Key, u64>,
    /// The body last asked for, drawn in parts: with the bytes it would
    /// hold whole, and the size of its parts.
    pieced: Option<(Key, u64, usize)>,
    drawn: u64,
}

/// Draws the bodies asked for, the latest first, then the rest in order,
/// each while it fits `room`. The body of the latest request (`latest`),
/// if it is one of them, is never held back: past the room it is drawn in
/// parts within what is left of it; with no room left at all, the floor,
/// whole when it is no more than one part, otherwise in parts of one part
/// each. No other body passes the room, even when the latest request is
/// an item read that draws within its own allowance.
fn fit(
    bodies: &[(Key, u64)],
    (asked, latest): (&HashMap<Key, u64>, Option<&Key>),
    room: u64,
) -> Fit {
    let mut order: Vec<&(Key, u64)> = bodies.iter().collect();
    // Stable: the rest keep their order after the bodies asked for.
    order.sort_by_key(|(key, _)| Reverse(asked.get(key).copied()));
    let mut fit = Fit::default();
    for (key, bytes) in order.iter().copied() {
        let left = room.saturating_sub(fit.drawn);
        if *bytes <= left {
            fit.drawn = fit.drawn.saturating_add(*bytes);
            continue;
        }
        if latest != Some(key) {
            fit.held_back.insert(key.clone(), *bytes);
            continue;
        }
        match piece_size(left) {
            None if *bytes <= PART as u64 => fit.drawn = fit.drawn.saturating_add(*bytes),
            size => {
                let size = size.unwrap_or(PIECE);
                fit.drawn = fit.drawn.saturating_add(pieced_bytes(size));
                fit.pieced = Some((key.clone(), *bytes, size));
            }
        }
    }
    fit
}

/// The size of the parts a body is drawn in within `left` bytes, a part,
/// its drawn copy and the index of where its parts start: at most
/// [`PIECE`]. `None` when `left` holds no part at all, which is the floor.
pub(super) fn piece_size(left: u64) -> Option<usize> {
    (left >= MIN_WINDOW).then(|| {
        let half = (left - INDEX_BYTES as u64) / 2;
        usize::try_from(half).unwrap_or(usize::MAX).min(PIECE)
    })
}

/// What a body drawn in parts of `size` bytes counts in the plan: a part,
/// its copy and the most its index keeps.
fn pieced_bytes(size: usize) -> u64 {
    (2 * size + INDEX_BYTES) as u64
}

/// The body of `key` drawn in parts of `size`: the one drawn before when
/// it is the same body cut the same way, so it is scanned only once.
fn cut_piece(
    selection: &Selection,
    (key, bytes, size): (Key, u64, usize),
    old: Option<Pieced>,
) -> Option<Pieced> {
    let version = selection.measures.get(&key)?.version;
    let entry = entry_of(&selection.join, &key)?;
    let held = selection.loaded.get(&key).map(|loaded| &loaded.held);
    let blocks = entry_blocks(entry, held);
    Some(cut(&blocks, (key, version), (size, bytes), old))
}

/// Shows part `wanted` of the body of `key` drawn in parts, in the plan
/// or within its read's reservation: that part alone is written out.
pub(super) fn show_piece(selection: &mut Selection, key: &Key, wanted: usize) {
    let Selection {
        join, loaded, plan, ..
    } = selection;
    if let Some(pieced) = plan.pieced.as_mut().filter(|pieced| pieced.key == *key) {
        if let Some(entry) = entry_of(join, key) {
            let held = loaded.get(key).map(|loaded| &loaded.held);
            pieced.show(&entry_blocks(entry, held), wanted);
        }
        return;
    }
    let Some(Loaded {
        held,
        window: Window::Pieced(pieced),
        ..
    }) = loaded.get_mut(key)
    else {
        return;
    };
    match held {
        Held::Whole(read) => pieced.show(&whole_blocks(read), wanted),
        Held::Parts(parts) => pieced.show(&[page_block(&parts.page.data)], wanted),
        Held::Loading | Held::Failed(_) => {}
    }
}

/// The entry `key` in the snapshot.
fn entry_of<'a>(join: &'a ActivityJoin, (turn, item): &Key) -> Option<&'a ActivityEntry> {
    join.activity()?
        .turns
        .iter()
        .find(|found| found.summary.id == *turn)?
        .items
        .iter()
        .find(|entry| entry.base().id == *item)
}

/// The version of the body of `key` as drawn: of what its read holds when
/// it is read whole, otherwise as last measured.
fn body_version(selection: &Selection, key: &Key) -> Version {
    match selection.loaded.get(key) {
        Some(loaded) if matches!(loaded.held, Held::Whole(_)) => {
            read_version(selection.epoch, loaded)
        }
        _ => selection
            .measures
            .get(key)
            .map_or((selection.epoch, 0, 0), |measure| measure.version),
    }
}

/// Whether the body of `key` is drawn block by block, so what is prepared
/// for it is kept: by the plan, or within its read's reservation.
fn drawn_in_blocks(selection: &Selection, key: &Key) -> bool {
    match selection
        .loaded
        .get(key)
        .map(|loaded| (&loaded.held, &loaded.window))
    {
        // A read draws within its allowance, whatever the plan.
        Some((Held::Whole(_) | Held::Parts(_), window)) => !matches!(window, Window::Pieced(_)),
        _ => {
            let plan = &selection.plan;
            selection.measures.contains_key(key)
                && !plan.held_back.contains_key(key)
                && plan.pieced.as_ref().is_none_or(|pieced| pieced.key != *key)
        }
    }
}

/// Lets go of what is prepared for the bodies no longer drawn block by
/// block, or whose version changed.
fn forget_prepared(selection: &mut Selection) {
    let keep: HashMap<Key, Version> = selection
        .prepared
        .borrow()
        .keys()
        .filter(|key| drawn_in_blocks(selection, key))
        .map(|key| (key.clone(), body_version(selection, key)))
        .collect();
    selection
        .prepared
        .get_mut()
        .retain(|key, (version, _)| keep.get(key) == Some(version));
}

/// What is prepared of the blocks of `key`'s body while it is drawn
/// block by block, kept between frames and counted in what the body
/// holds.
pub(super) fn prepared(selection: &Selection, key: &Key) -> Rc<Prepare> {
    let version = body_version(selection, key);
    let mut prepared = selection.prepared.borrow_mut();
    let entry = prepared
        .entry(key.clone())
        .or_insert_with(|| (version, Rc::default()));
    if entry.0 != version {
        *entry = (version, Rc::new(RefCell::default()));
    }
    entry.1.clone()
}

/// Asks for the body of `key`, makes room for it and draws it: a
/// person's request, so the latest ([`stamp`]).
pub(super) fn ask(selection: &mut Selection, key: &Key) {
    stamp(selection, key);
    make_room(selection, 0, Some(key));
    replan(selection);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{INDEX_BYTES, Key, MIN_PIECE, MIN_WINDOW, PART, PIECE, fit, piece_size};

    fn key(item: &str) -> Key {
        ("t".to_owned(), item.to_owned())
    }

    /// Bodies are drawn in order while they fit; one that would pass the
    /// room is held back, and a smaller one after it may still fit.
    #[test]
    fn bodies_are_drawn_in_order_within_the_room() {
        let bodies = [(key("a"), 40), (key("b"), 50), (key("c"), 10)];
        let plan = fit(&bodies, (&HashMap::new(), None), 60);
        assert_eq!(plan.drawn, 50);
        assert_eq!(plan.held_back, HashMap::from([(key("b"), 50)]));
    }

    /// Round 5 thermo (P1/P3), SUB decision `33ccecf6` item 1: the bodies
    /// asked for come first, the latest first; with room left, the latest
    /// is drawn in parts within it, a part and its copy, never past it.
    #[test]
    fn the_body_last_asked_for_is_drawn_first_and_within_any_room_left() {
        let bodies = [(key("a"), 40), (key("b"), 50), (key("c"), 10_000)];
        let asked = HashMap::from([(key("b"), 1), (key("c"), 2)]);
        let room = 60 + INDEX_BYTES as u64;
        let plan = fit(&bodies, (&asked, Some(&key("c"))), room);
        assert_eq!(plan.pieced, Some((key("c"), 10_000, 30)));
        assert_eq!(plan.drawn, room);
        assert_eq!(
            plan.held_back,
            HashMap::from([(key("a"), 40), (key("b"), 50)])
        );
    }

    /// SUB decision `8357ec55` §2: with no room left, the floor: the body
    /// last asked for draws whole up to one part, and one part at a time
    /// past it; a body asked for earlier never passes the room.
    #[test]
    fn only_the_body_last_asked_for_passes_no_room_and_by_one_part() {
        let bodies = [(key("a"), 40), (key("b"), 50)];
        let asked = HashMap::from([(key("a"), 1), (key("b"), 2)]);
        let plan = fit(&bodies, (&asked, Some(&key("b"))), 0);
        assert_eq!(plan.drawn, 50);
        assert!(plan.pieced.is_none());
        assert_eq!(plan.held_back, HashMap::from([(key("a"), 40)]));

        let big = PART as u64 + 1;
        let plan = fit(&[(key("b"), big)], (&asked, Some(&key("b"))), 0);
        assert_eq!(plan.pieced, Some((key("b"), big, PIECE)));
        assert_eq!(plan.drawn, 2 * PART as u64);
    }

    /// Round 4 review (P3), SUB decision `0c13b784` §3: a body asked for
    /// that is larger than a positive room is not admitted whole; it is
    /// drawn in parts, a part and its drawn copy counted.
    #[test]
    fn a_body_asked_for_past_the_room_is_drawn_in_parts() {
        let big = 10 * PART as u64;
        let bodies = [(key("a"), 40), (key("b"), big)];
        let asked = HashMap::from([(key("b"), 1)]);
        let plan = fit(&bodies, (&asked, Some(&key("b"))), 3 * PART as u64);
        assert_eq!(plan.pieced, Some((key("b"), big, PIECE)));
        assert_eq!(plan.drawn, 2 * PART as u64 + 40);
        assert!(plan.held_back.is_empty());
    }

    /// Round 6 thermo (P1), SUB decision `4311830a` item 1: the floor
    /// belongs to the latest request; when that is an item read, which is
    /// not planned here, no earlier body passes the room in its place.
    #[test]
    fn the_floor_never_passes_to_a_body_asked_for_before_the_latest_read() {
        let asked = HashMap::from([(key("a"), 1), (key("read"), 2)]);
        let plan = fit(&[(key("a"), 40)], (&asked, Some(&key("read"))), 0);
        let held_back = HashMap::from([(key("a"), 40)]);
        assert_eq!((plan.drawn, plan.held_back), (0, held_back));
    }

    /// A part, its copy and the index of where the parts start fit what
    /// is left, a part and its copy at most two parts' bytes with the
    /// index; less than a character, its copy and the index is the floor
    /// (SUB decision `4311830a` item 5).
    #[test]
    fn parts_fit_what_is_left_beside_their_index_up_to_one_part() {
        assert_eq!(piece_size(0), None);
        assert_eq!(piece_size(MIN_WINDOW - 1), None);
        assert_eq!(piece_size(MIN_WINDOW), Some(MIN_PIECE));
        assert_eq!(piece_size(1001 + INDEX_BYTES as u64), Some(500));
        assert_eq!(piece_size(u64::MAX), Some(PIECE));
        assert_eq!(2 * PIECE + INDEX_BYTES, 2 * PART);
    }
}
