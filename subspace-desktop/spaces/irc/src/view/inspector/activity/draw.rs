//! Which item bodies the view draws (SUB decisions `a17870f4` §5,
//! `ed71a299` and `0c13b784`). What the open turns draw counts against
//! the one budget beside the inline content and the items read, in the
//! UTF-8 bytes it mounts: each block as it draws, markers and all, or one
//! part when it is long, plus the formatted text kept for it (an unknown
//! item's indented JSON, an error); an item read on demand what is shown
//! of it. Collapsed turns build no bodies, so they draw nothing.
//!
//! Each body is measured once per version of its item, by turn, item and
//! `seq` (and, for an item read on demand, what is held of it), and the
//! plan below is worked out again only when the snapshot, the items read,
//! the turns opened or the bodies asked for change: drawing a frame
//! neither measures nor formats anything already drawn.
//!
//! The order: the bodies the person asked for come first, the latest
//! first; then the open turns, the most recently opened first, and their
//! items in order. Each is drawn while it fits the room the budget leaves
//! beside the inline content and the items read; one that would pass it
//! shows its label, a caption with its size saying it is not drawn to
//! stay within the view's memory budget, and **Show**. Nothing is cut:
//! every body is one click away. **Show** first makes room, letting go of
//! the least recently opened items read, then of the bodies asked for
//! earlier, the least recent first, until what is held and drawn fits
//! beside the body asked for. If the body still does not fit, it is drawn
//! in parts of at most 64 KiB with First, Previous, Next and Last
//! ([`super::pieces`]), never hidden and never cut; a body of no more than
//! one part draws whole. Reading an item asks for its body too.
//!
//! The floor: inline content is the snapshot itself and is never let go.
//! When it alone fills the budget, the view holds one part (at most
//! 64 KiB) of the item last asked for, and its drawn copy, besides it;
//! while a part of an item read on demand is read, the page coming in.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::rc::Rc;

use subspace_client_fleet::activity::{ActivityEntry, ActivityTurn};

use super::blocks::entry_blocks;
use super::load::{held, room};
use super::parts::{PART, Prepare};
use super::pieces::{Piece, piece};
use super::size::body_bytes;
use super::turn::is_open;
use super::{Loaded, Selection};

mod cards;

pub(super) use cards::{held_back, pieced, pieced_parts};

/// An item, by its turn and item IDs.
pub(super) type Key = (String, String);

/// One version of an item's body: its `seq`, and for an item read on
/// demand, the version of what is held of it ([`Loaded::version`]).
pub(super) type Version = (u64, u64);

/// What a body drawn in parts counts: the part kept and its drawn copy.
const PIECED: u64 = 2 * PART as u64;

/// An item's body as last measured.
pub(super) struct Measure {
    version: Version,
    bytes: u64,
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

/// The body last asked for, too large for the room even once room was
/// made, and the part of it shown.
pub(super) struct Pieced {
    pub(super) key: Key,
    version: Version,
    /// The part asked for, which `piece` may stop short of.
    asked: usize,
    /// The bytes it would hold drawn whole.
    pub(super) bytes: u64,
    pub(super) piece: Piece,
}

/// Measures what changed and works out again which bodies the open turns
/// draw within the room left. Run after every change to what they draw,
/// never while drawing.
pub(super) fn replan(selection: &mut Selection) {
    measure(selection);
    let bodies = bodies(selection);
    let room = room(selection).saturating_sub(held(selection));
    let fit = fit(&bodies, &selection.asked, room);
    let old = selection.plan.pieced.take();
    let pieced = fit
        .pieced
        .and_then(|(key, bytes)| cut_piece(selection, (key, bytes), old));
    selection.plan = Plan {
        held_back: fit.held_back,
        pieced,
        #[cfg(test)]
        drawn: fit.drawn,
    };
    forget_prepared(selection);
}

/// The version of the body of `entry`.
fn version(entry: &ActivityEntry, loaded: Option<&Loaded>) -> Version {
    (entry.base().seq, loaded.map_or(0, |loaded| loaded.version))
}

/// Measures each body of the open turns whose version changed since it
/// was last measured, and forgets those no longer drawn.
fn measure(selection: &mut Selection) {
    selection.measures = measures(selection);
}

/// Each body of the open turns as measured: again only when its version
/// changed.
fn measures(selection: &Selection) -> HashMap<Key, Measure> {
    open_entries(selection)
        .into_iter()
        .map(|(key, entry)| {
            let loaded = selection.loaded.get(&key);
            let version = version(entry, loaded);
            let bytes = match selection.measures.get(&key) {
                Some(measure) if measure.version == version => measure.bytes,
                _ => body_bytes(&entry_blocks(entry, loaded.map(|loaded| &loaded.held))),
            };
            (key, Measure { version, bytes })
        })
        .collect()
}

/// The items of the open turns, the most recently opened turn first, and
/// its items in order.
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

/// The open turns, the most recently opened first.
fn open_turns<'a>(selection: &Selection, turns: &'a [ActivityTurn]) -> Vec<&'a ActivityTurn> {
    let mut open: Vec<&ActivityTurn> = turns
        .iter()
        .rev()
        .filter(|turn| is_open(selection, &turn.summary))
        .collect();
    // Stable: turns open by default keep their order, newest first.
    open.sort_by_key(|turn| Reverse(selection.turn_opened.get(&turn.summary.id).copied()));
    open
}

/// Every body the open turns would draw, as measured, in their order.
fn bodies(selection: &Selection) -> Vec<(Key, u64)> {
    open_entries(selection)
        .into_iter()
        .map(|(key, _)| {
            let bytes = selection
                .measures
                .get(&key)
                .map_or(0, |measure| measure.bytes);
            (key, bytes)
        })
        .collect()
}

/// Which bodies a plan draws.
#[derive(Default)]
struct Fit {
    held_back: HashMap<Key, u64>,
    /// The body last asked for, drawn in parts, with the bytes it would
    /// hold whole.
    pieced: Option<(Key, u64)>,
    drawn: u64,
}

/// Draws the bodies asked for, the latest first, then the rest in order,
/// each while it fits `room`. The body last asked for is never held back:
/// past the room it is drawn in parts, or whole when it is no more than
/// one part.
fn fit(bodies: &[(Key, u64)], asked: &HashMap<Key, u64>, room: u64) -> Fit {
    let mut order: Vec<&(Key, u64)> = bodies.iter().collect();
    // Stable: the rest keep their order after the bodies asked for.
    order.sort_by_key(|(key, _)| Reverse(asked.get(key).copied()));
    let latest = order
        .first()
        .filter(|(key, _)| asked.contains_key(key))
        .map(|(key, _)| key.clone());
    let mut fit = Fit::default();
    for (key, bytes) in order.iter().copied() {
        let floor = latest.as_ref() == Some(key);
        if fit.drawn.saturating_add(*bytes) <= room || (floor && *bytes <= PART as u64) {
            fit.drawn = fit.drawn.saturating_add(*bytes);
        } else if floor {
            fit.drawn = fit.drawn.saturating_add(PIECED);
            fit.pieced = Some((key.clone(), *bytes));
        } else {
            fit.held_back.insert(key.clone(), *bytes);
        }
    }
    fit
}

/// The body of `key` drawn in parts, the part asked for shown: the part
/// drawn before when neither changed, so it is formatted only once.
fn cut_piece(
    selection: &Selection,
    (key, bytes): (Key, u64),
    old: Option<Pieced>,
) -> Option<Pieced> {
    let version = selection.measures.get(&key)?.version;
    let asked = asked_piece(selection, &key);
    old.filter(|old| old.key == key && old.version == version && old.asked == asked)
        .map(|old| Pieced { bytes, ..old })
        .or_else(|| new_piece(selection, (key, version), (asked, bytes)))
}

/// The part asked for of the body of `key`: the first unless one was.
fn asked_piece(selection: &Selection, key: &Key) -> usize {
    selection
        .piece
        .as_ref()
        .filter(|(pieced, _)| pieced == key)
        .map_or(0, |(_, piece)| *piece)
}

/// Cuts part `asked` out of the body of `key`, of `bytes` drawn whole.
fn new_piece(
    selection: &Selection,
    (key, version): (Key, Version),
    (asked, bytes): (usize, u64),
) -> Option<Pieced> {
    let entry = entry_of(selection, &key)?;
    let held = selection.loaded.get(&key).map(|loaded| &loaded.held);
    let piece = piece(&entry_blocks(entry, held), asked);
    Some(Pieced {
        key,
        version,
        asked,
        bytes,
        piece,
    })
}

/// The entry `key` in the snapshot.
fn entry_of<'a>(selection: &'a Selection, (turn, item): &Key) -> Option<&'a ActivityEntry> {
    selection
        .join
        .activity()?
        .turns
        .iter()
        .find(|found| found.summary.id == *turn)?
        .items
        .iter()
        .find(|entry| entry.base().id == *item)
}

/// Lets go of what is prepared for the bodies no longer drawn whole, or
/// whose item changed.
fn forget_prepared(selection: &mut Selection) {
    let (measures, plan) = (&selection.measures, &selection.plan);
    selection.prepared.get_mut().retain(|key, (version, _)| {
        measures
            .get(key)
            .is_some_and(|measure| measure.version == *version)
            && !plan.held_back.contains_key(key)
            && plan.pieced.as_ref().is_none_or(|pieced| pieced.key != *key)
    });
}

/// What is prepared of the blocks of `key`'s body while it is drawn
/// whole, kept between frames and counted in what the body holds.
pub(super) fn prepared(selection: &Selection, key: &Key) -> Rc<Prepare> {
    let version = selection
        .measures
        .get(key)
        .map_or((0, 0), |measure| measure.version);
    let mut prepared = selection.prepared.borrow_mut();
    let entry = prepared
        .entry(key.clone())
        .or_insert_with(|| (version, Rc::default()));
    if entry.0 != version {
        *entry = (version, Rc::new(RefCell::default()));
    }
    entry.1.clone()
}

/// Asks for the body of `key`, makes room for it and draws it.
pub(super) fn ask(selection: &mut Selection, key: &Key) {
    selection.opened += 1;
    selection.asked.insert(key.clone(), selection.opened);
    measure(selection);
    make_room(selection, key);
    replan(selection);
}

/// Lets go of the least recently opened items read, then of the bodies
/// asked for earlier, the least recent first, other than `key`'s, until
/// what is held and drawn fits the budget beside its body, or nothing
/// else is left. The bodies are listed once.
fn make_room(selection: &mut Selection, key: &Key) {
    let bodies: HashMap<Key, u64> = bodies(selection).into_iter().collect();
    let wanted = bodies.get(key).copied().unwrap_or(0);
    let mut asked: HashMap<Key, u64> = selection
        .asked
        .keys()
        .filter(|asked| *asked != key)
        .map(|asked| (asked.clone(), bodies.get(asked).copied().unwrap_or(0)))
        .collect();
    loop {
        let used = held(selection)
            .saturating_add(asked.values().sum())
            .saturating_add(wanted);
        if used <= room(selection) {
            return;
        }
        let oldest_read = selection
            .loaded
            .iter()
            .filter(|(read, _)| *read != key)
            .min_by_key(|(_, loaded)| loaded.opened)
            .map(|(read, _)| read.clone());
        if let Some(read) = oldest_read {
            selection.loaded.remove(&read);
            // Let go, it draws its load action alone.
            if let Some(bytes) = asked.get_mut(&read) {
                *bytes = 0;
            }
            continue;
        }
        let oldest_asked = asked
            .keys()
            .min_by_key(|asked| selection.asked.get(*asked).copied())
            .cloned();
        let Some(oldest) = oldest_asked else {
            return;
        };
        asked.remove(&oldest);
        selection.asked.remove(&oldest);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{Key, PART, PIECED, fit};

    fn key(item: &str) -> Key {
        ("t".to_owned(), item.to_owned())
    }

    /// Bodies are drawn in order while they fit; one that would pass the
    /// room is held back, and a smaller one after it may still fit.
    #[test]
    fn bodies_are_drawn_in_order_within_the_room() {
        let bodies = [(key("a"), 40), (key("b"), 50), (key("c"), 10)];
        let plan = fit(&bodies, &HashMap::new(), 60);
        assert_eq!(plan.drawn, 50);
        assert_eq!(plan.held_back, HashMap::from([(key("b"), 50)]));
    }

    /// The bodies asked for come first, the latest first; the latest is
    /// never held back, and one of no more than a part draws whole even
    /// past the room: the floor.
    #[test]
    fn the_body_last_asked_for_is_drawn_first_and_whole_up_to_a_part() {
        let bodies = [(key("a"), 40), (key("b"), 50), (key("c"), 100)];
        let asked = HashMap::from([(key("b"), 1), (key("c"), 2)]);
        let plan = fit(&bodies, &asked, 60);
        assert_eq!(plan.drawn, 100);
        assert!(plan.pieced.is_none());
        assert_eq!(
            plan.held_back,
            HashMap::from([(key("a"), 40), (key("b"), 50)])
        );
    }

    /// Round 4 review (P3), SUB decision `0c13b784` §3: a body asked for
    /// that is larger than a positive room is not admitted whole; it is
    /// drawn in parts, a part and its drawn copy counted.
    #[test]
    fn a_body_asked_for_past_the_room_is_drawn_in_parts() {
        let big = 10 * PART as u64;
        let bodies = [(key("a"), 40), (key("b"), big)];
        let asked = HashMap::from([(key("b"), 1)]);
        let plan = fit(&bodies, &asked, 3 * PART as u64);
        assert_eq!(plan.pieced, Some((key("b"), big)));
        assert_eq!(plan.drawn, PIECED + 40);
        assert!(plan.held_back.is_empty());
    }
}
