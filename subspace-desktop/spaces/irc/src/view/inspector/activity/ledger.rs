//! The one ledger of what the view holds and which request comes first
//! (SUB decision `4311830a` item 1). Admission, publication, a refresh and
//! the plan of what the open turns draw ([`super::draw`]) all count
//! against it, and only a person's request changes priority ([`stamp`]).
//!
//! Each item read counts its steady charge, what it holds and the window
//! it draws, within the steady allowance it was granted, and beside it the
//! temporary allowance for what is coming in: the page of a part being
//! read. Each body asked for counts what it draws. Room is made by letting
//! go of the lowest priority first, items read and bodies asked for alike,
//! never the request being made.
//!
//! The floor (SUB decision `8357ec55` §2) belongs to the latest request,
//! whatever it is. When what is left beside everything else does not hold
//! that request's read, its window is sized to the room left, when that
//! holds what it holds and a part of its window; with no room left, to the
//! floor's steady allowance, a part and its window ([`PAGE_READ`]), beside
//! at most a page coming in. A read not even the floor holds is let go.

use std::collections::HashMap;

use super::Selection;
use super::draw::{Key, MIN_PIECE, measure};
use super::load::{PAGE, PAGE_READ, refit};
use super::parts::PART;

/// The bytes the budget leaves for items once the inline content is
/// counted.
pub(super) fn room(selection: &Selection) -> u64 {
    selection.budget.saturating_sub(selection.inline)
}

/// The bytes the items read count: what each holds and draws, and what
/// comes in for it.
pub(super) fn held(selection: &Selection) -> u64 {
    selection
        .loaded
        .values()
        .map(|loaded| loaded.charge())
        .sum()
}

/// Records a person's request for `key` as the latest: the only change to
/// priority, never made when a read completes.
pub(super) fn stamp(selection: &mut Selection, key: &Key) -> u64 {
    selection.opened += 1;
    selection.asked.insert(key.clone(), selection.opened);
    selection.opened
}

/// The latest request: the one the floor belongs to.
pub(super) fn latest(selection: &Selection) -> Option<&Key> {
    selection
        .asked
        .iter()
        .max_by_key(|(_, asked)| **asked)
        .map(|(key, _)| key)
}

/// What each body asked for draws in the plan, other than `keep`'s. A
/// body an item read draws whole is not planned, so it counts in the read.
fn asked_bodies(selection: &Selection, keep: Option<&Key>) -> HashMap<Key, u64> {
    selection
        .asked
        .keys()
        .filter(|key| Some(*key) != keep)
        .filter_map(|key| Some((key.clone(), selection.measures.get(key)?.bytes)))
        .collect()
}

/// The lowest priority other than `keep`: of the items read and the bodies
/// asked for (`asked`), the least recently asked for.
fn lowest(selection: &Selection, asked: &HashMap<Key, u64>, keep: Option<&Key>) -> Option<Key> {
    let priority = |key: &Key| selection.asked.get(key).copied().unwrap_or(0);
    selection
        .loaded
        .keys()
        .chain(asked.keys())
        .filter(|key| Some(*key) != keep)
        .min_by_key(|key| (priority(key), (*key).clone()))
        .cloned()
}

/// Lets go of `key`: its read, cancelled, which then draws its load
/// action alone; or, when it is a body asked for, the request for it.
fn let_go(selection: &mut Selection, asked: &mut HashMap<Key, u64>, key: &Key) {
    asked.remove(key);
    if selection.loaded.remove(key).is_none() {
        selection.asked.remove(key);
    }
}

/// What everything other than `keep` counts, the bodies asked for
/// (`asked`) and `wanted` more besides.
fn beside(
    selection: &Selection,
    asked: &HashMap<Key, u64>,
    (keep, wanted): (Option<&Key>, u64),
) -> u64 {
    let kept = keep
        .and_then(|keep| selection.loaded.get(keep))
        .map_or(0, |loaded| loaded.charge());
    selection
        .loaded
        .values()
        .map(|loaded| loaded.charge())
        .sum::<u64>()
        .saturating_sub(kept)
        .saturating_add(asked.values().sum())
        .saturating_add(wanted)
}

/// Lets go of the lowest priority, other than `keep`, until what is held
/// and drawn fits the room beside `wanted` more and `keep`'s read and
/// body, or nothing else is left; then fits `keep`'s read into what is
/// left, or the floor ([`fit_kept`]).
pub(super) fn make_room(selection: &mut Selection, wanted: u64, keep: Option<&Key>) {
    measure(selection);
    let wanted = wanted.saturating_add(body(selection, keep));
    let mut asked = asked_bodies(selection, keep);
    let room = room(selection);
    while beside(selection, &asked, (keep, wanted)).saturating_add(kept(selection, keep)) > room {
        let Some(lowest) = lowest(selection, &asked, keep) else {
            break;
        };
        let_go(selection, &mut asked, &lowest);
    }
    if let Some(keep) = keep {
        let left = room.saturating_sub(beside(selection, &asked, (Some(keep), wanted)));
        fit_kept(selection, keep, left);
    }
}

/// What `keep`'s body draws in the plan, if it is planned.
fn body(selection: &Selection, keep: Option<&Key>) -> u64 {
    keep.and_then(|keep| selection.measures.get(keep))
        .map_or(0, |measure| measure.bytes)
}

/// What `keep`'s read counts.
fn kept(selection: &Selection, keep: Option<&Key>) -> u64 {
    keep.and_then(|keep| selection.loaded.get(keep))
        .map_or(0, |loaded| loaded.charge())
}

/// The room left beside everything other than `keep`'s read, the bodies
/// asked for and `keep`'s own body included.
pub(super) fn left(selection: &Selection, keep: &Key) -> u64 {
    let asked = asked_bodies(selection, Some(keep));
    let wanted = body(selection, Some(keep));
    room(selection).saturating_sub(beside(selection, &asked, (Some(keep), wanted)))
}

/// What a page's window may draw beside it, given the `left` bytes the
/// room leaves once the page is counted: what is left, up to a part, when
/// that holds a part at all; otherwise the floor's part.
pub(super) fn window_allowance(left: u64) -> u64 {
    if left >= 2 * MIN_PIECE as u64 {
        left.min(PART as u64)
    } else {
        PART as u64
    }
}

/// Fits the read `keep`, the request being made or the latest, into the
/// `left` bytes the room leaves it: unchanged when it fits; otherwise its
/// window sized to what is left, or the floor ([`floor_allowance`]); let
/// go when not even the floor holds it.
fn fit_kept(selection: &mut Selection, keep: &Key, left: u64) {
    let Some(loaded) = selection.loaded.get(keep) else {
        return;
    };
    if loaded.charge() <= left {
        return;
    }
    match floor_allowance(loaded.charges(), left) {
        Some(allowance) => refit(selection, keep, allowance),
        None => {
            selection.loaded.remove(keep);
        }
    }
}

/// The steady allowance of a read that passes the `left` bytes the room
/// leaves it, from what it holds (`raw`), what comes in for it
/// (`incoming`) and, while its first read is under way, its allowance
/// (`reading`): the room left beside what comes in, when that holds what
/// it holds and a part of its window; otherwise the floor's, a part and
/// its window, beside at most a page coming in. `None` when not even the
/// floor holds it.
fn floor_allowance((raw, incoming, reading): (u64, u64, Option<u64>), left: u64) -> Option<u64> {
    if let Some(allowance) = reading {
        return (allowance <= PAGE_READ).then_some(allowance);
    }
    let need = raw.saturating_add(2 * MIN_PIECE as u64);
    let steady = left.saturating_sub(incoming);
    if steady >= need {
        return Some(steady);
    }
    (incoming <= PAGE && need <= PAGE_READ).then_some(PAGE_READ)
}

#[cfg(test)]
mod tests {
    use super::{MIN_PIECE, PAGE, PAGE_READ, PART, floor_allowance, window_allowance};

    /// SUB decision `4311830a` item 1: a window is sized to the positive
    /// room left, and to the floor only when none is.
    #[test]
    fn a_window_is_sized_to_the_room_left_and_to_the_floor_only_without_any() {
        assert_eq!(window_allowance(4_464), 4_464);
        assert_eq!(window_allowance(u64::MAX), PART as u64);
        assert_eq!(window_allowance(0), PART as u64);
        assert_eq!(window_allowance(2 * MIN_PIECE as u64 - 1), PART as u64);
    }

    /// SUB decision `4311830a` item 1: the floor keeps the steady
    /// allowance of a part and its window apart from the page coming in.
    #[test]
    fn the_floor_keeps_a_part_and_its_window_beside_a_page_coming_in() {
        // A part shown, with a page coming in, and no room left.
        assert_eq!(floor_allowance((PAGE, PAGE, None), 0), Some(PAGE_READ));
        // Room left beside the page coming in: sized to it.
        assert_eq!(
            floor_allowance((PAGE, PAGE, None), 2 * PAGE + 100),
            Some(PAGE + 100)
        );
        // More than a part and its window held: not even the floor.
        assert_eq!(floor_allowance((PAGE_READ, 0, None), 0), None);
        // A whole read coming in is no page.
        assert_eq!(floor_allowance((0, 2 * PAGE, None), 0), None);
        // A first read under way keeps its allowance only within the floor.
        assert_eq!(floor_allowance((0, 0, Some(PAGE_READ)), 0), Some(PAGE_READ));
        assert_eq!(floor_allowance((0, 0, Some(PAGE_READ + 1)), 0), None);
    }
}
