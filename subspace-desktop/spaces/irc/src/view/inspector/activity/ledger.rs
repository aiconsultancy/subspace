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
//! never the request being made, and never for a read continuing on its
//! own or arriving a request newer than its own (SUB decisions `084f5236`
//! item 4 and `a884e4c6`).
//!
//! The floor (SUB decision `8357ec55` §2) belongs to the latest request,
//! whatever it is. When what is left beside everything else does not hold
//! that request's read, its window is sized to the room left, when that
//! holds what it holds and a part of its window; with no room left, to the
//! floor's steady allowance, a part and its window ([`PAGE_READ`]), beside
//! at most a page coming in. A read replacing what is shown counts only
//! what it asks for, a page for a part; a whole one the floor cannot hold
//! beside what is shown is withdrawn, and what is shown stays (SUB
//! decision `084f5236` item 2). A read not even the floor holds is let go.

use std::collections::HashMap;

use super::draw::{Key, MIN_WINDOW, measure};
use super::load::{PAGE, PAGE_READ, give_back, refit};
use super::parts::PART;
use super::{Loaded, Selection};

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
/// asked for (`asked`), the least recently asked for, and never one asked
/// for since `keep` was. A request being made is the newest, so this binds
/// only room made for no one's request, a read arriving or read again,
/// which never displaces a newer request (SUB decision `a884e4c6`).
fn lowest(selection: &Selection, asked: &HashMap<Key, u64>, keep: Option<&Key>) -> Option<Key> {
    let priority = |key: &Key| selection.asked.get(key).copied().unwrap_or(0);
    let ceiling = keep.map_or(u64::MAX, priority);
    selection
        .loaded
        .keys()
        .chain(asked.keys())
        .filter(|key| Some(*key) != keep && priority(key) <= ceiling)
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

/// Makes room for `wanted` more beside `keep`'s read and body: first by
/// giving back every read's growth past its allowance (SUB decision
/// `feb0d839`), then by letting go of the lowest priority, other than
/// `keep`, until what is held and drawn fits the room, or nothing else is
/// left; then fits `keep`'s read into what is left, or the floor
/// ([`fit_kept`]).
pub(super) fn make_room(selection: &mut Selection, wanted: u64, keep: Option<&Key>) {
    measure(selection);
    let wanted = wanted.saturating_add(body(selection, keep));
    let mut asked = asked_bodies(selection, keep);
    let room = room(selection);
    let over = |selection: &Selection, asked: &HashMap<Key, u64>| {
        beside(selection, asked, (keep, wanted)).saturating_add(kept(selection, keep)) > room
    };
    if over(selection, &asked) {
        give_back(selection);
    }
    while over(selection, &asked) {
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

/// What `keep`'s body draws in the plan, if it is planned and keeps the
/// floor. None when a read of it is under way or holds what was read: that
/// read is fitted first, and what its placeholder names draws only within
/// the room left beside it (SUB decision `084f5236` item 1).
fn body(selection: &Selection, keep: Option<&Key>) -> u64 {
    let keep = keep.filter(|keep| !selection.loaded.get(*keep).is_some_and(Loaded::presents));
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
fn left(selection: &Selection, keep: &Key) -> u64 {
    let asked = asked_bodies(selection, Some(keep));
    let wanted = body(selection, Some(keep));
    room(selection).saturating_sub(beside(selection, &asked, (Some(keep), wanted)))
}

/// The room `keep`'s read may hold and draw in beside everything else,
/// the bodies asked for and its own body included, measured again: what
/// its window may take without growing into room anything else holds.
pub(super) fn room_beside(selection: &mut Selection, keep: &Key) -> u64 {
    measure(selection);
    left(selection, keep)
}

/// The room left for `keep`'s next read beside everything else, what its
/// read shows now included.
pub(super) fn spare(selection: &Selection, keep: &Key) -> u64 {
    left(selection, keep).saturating_sub(kept(selection, Some(keep)))
}

/// The room `key` may take beside every request newer than its own, the
/// items read and bodies asked for since, counted as making room counts
/// them, a read's placeholder beside the read, and its own body: room it
/// could take letting go of nothing newer. A read that continues on its
/// own, or arrives, is no request, so it never displaces a newer one (SUB
/// decisions `084f5236` item 4 and `a884e4c6`).
pub(super) fn room_before_newer(selection: &Selection, key: &Key) -> u64 {
    let priority = |other: &Key| selection.asked.get(other).copied().unwrap_or(0);
    let own = priority(key);
    let newer = |other: &&Key| *other != key && priority(other) > own;
    let reads: u64 = selection
        .loaded
        .iter()
        .filter(|(other, _)| newer(other))
        .map(|(_, loaded)| loaded.charge())
        .sum();
    let bodies: u64 = asked_bodies(selection, Some(key))
        .iter()
        .filter(|(other, _)| newer(other))
        .map(|(_, bytes)| bytes)
        .sum();
    room(selection)
        .saturating_sub(reads)
        .saturating_sub(bodies)
        .saturating_sub(body(selection, Some(key)))
}

/// The room a read of `key` arriving may take letting go of nothing newer,
/// measured again ([`room_before_newer`]), and whether `key` is the latest
/// request, to which the floor belongs.
pub(super) fn arrival_room(selection: &mut Selection, key: &Key) -> (u64, bool) {
    measure(selection);
    let latest = latest(selection) == Some(key);
    (room_before_newer(selection, key), latest)
}

/// Whether `wanted` more for `key` fits the room it may take letting go of
/// nothing newer ([`room_before_newer`]).
pub(super) fn fits_before_newer(selection: &Selection, key: &Key, wanted: u64) -> bool {
    wanted <= room_before_newer(selection, key)
}

/// The steady allowance of a read replacing what is shown, granted
/// `granted` at admission, once it arrives holding `raw` bytes, within the
/// `room` it may take letting go of nothing newer ([`room_before_newer`]):
/// the latest request keeps its grant, the floor being its own; any other
/// read is granted no more than that room, when it holds what arrived and
/// a part of its window. `None` when it does not: the swap would displace
/// a newer request, so what is shown stays (SUB decision `a884e4c6`).
pub(super) fn swap_allowance(granted: u64, raw: u64, (room, latest): (u64, bool)) -> Option<u64> {
    if latest {
        return Some(granted);
    }
    (raw.saturating_add(MIN_WINDOW) <= room).then_some(granted.min(room))
}

/// What a page's window may draw beside it, given the `left` bytes the
/// room leaves once the page is counted: what is left, up to a part, when
/// that holds a part, its copy and their index at all; otherwise the
/// floor's part.
pub(super) fn window_allowance(left: u64) -> u64 {
    if left >= MIN_WINDOW {
        left.min(PART as u64)
    } else {
        PART as u64
    }
}

/// Fits the read `keep`, the request being made, the latest or a read
/// arriving, into the `left` bytes the room leaves it: unchanged when it
/// fits; otherwise its window sized to what is left, or, for the latest
/// request alone, the floor ([`kept_allowance`]). When that holds what it
/// shows but not beside the read under way for it, that read is withdrawn
/// and what it shows stays (SUB decision `084f5236` item 2); it is let go
/// only when not even that holds what it shows.
fn fit_kept(selection: &mut Selection, keep: &Key, left: u64) {
    let latest = latest(selection) == Some(keep);
    let Some(loaded) = selection.loaded.get_mut(keep) else {
        return;
    };
    if loaded.charge() <= left {
        return;
    }
    let fitted = |loaded: &Loaded| kept_allowance(loaded.charges(), (left, latest));
    if fitted(loaded).is_none() {
        loaded.withdraw();
    }
    match fitted(loaded) {
        Some(allowance) => refit(selection, keep, allowance),
        None => {
            selection.loaded.remove(keep);
        }
    }
}

/// The steady allowance of a read that passes the `left` bytes the room
/// leaves it: within the floor for the latest request
/// ([`floor_allowance`]), and only within what is left for any other read
/// ([`left_allowance`]). The floor belongs to the latest request alone
/// (SUB decision `a884e4c6`).
fn kept_allowance(charges: (u64, u64, Option<u64>), (left, latest): (u64, bool)) -> Option<u64> {
    if latest {
        floor_allowance(charges, left)
    } else {
        left_allowance(charges, left)
    }
}

/// The steady allowance of a read that passes the `left` bytes the room
/// leaves it, from what it holds (`raw`), what comes in for it
/// (`incoming`) and, while its first read is under way, its allowance
/// (`reading`): the room left beside what comes in, when that holds what
/// it holds and a part of its window. `None` when it does not.
fn left_allowance((raw, incoming, reading): (u64, u64, Option<u64>), left: u64) -> Option<u64> {
    if let Some(allowance) = reading {
        return (allowance <= left).then_some(allowance);
    }
    let steady = left.saturating_sub(incoming);
    (steady >= raw.saturating_add(MIN_WINDOW)).then_some(steady)
}

/// The steady allowance of the latest request's read that passes the
/// `left` bytes the room leaves it: what is left ([`left_allowance`]);
/// otherwise the floor's, a part and its window, beside at most a page
/// coming in, and while its first read is under way its allowance within
/// that. `None` when not even the floor holds it.
fn floor_allowance((raw, incoming, reading): (u64, u64, Option<u64>), left: u64) -> Option<u64> {
    if let Some(allowance) = reading {
        return (allowance <= PAGE_READ).then_some(allowance);
    }
    let need = raw.saturating_add(MIN_WINDOW);
    let floor = (incoming <= PAGE && need <= PAGE_READ).then_some(PAGE_READ);
    left_allowance((raw, incoming, None), left).or(floor)
}

#[cfg(test)]
mod tests {
    use super::{
        MIN_WINDOW, PAGE, PAGE_READ, PART, floor_allowance, kept_allowance, swap_allowance,
        window_allowance,
    };

    /// SUB decision `4311830a` item 1: a window is sized to the positive
    /// room left, and to the floor only when none is.
    #[test]
    fn a_window_is_sized_to_the_room_left_and_to_the_floor_only_without_any() {
        assert_eq!(window_allowance(4_464), 4_464);
        assert_eq!(window_allowance(u64::MAX), PART as u64);
        assert_eq!(window_allowance(0), PART as u64);
        assert_eq!(window_allowance(MIN_WINDOW - 1), PART as u64);
    }

    /// SUB decision `4311830a` item 1: the floor keeps the steady
    /// allowance of a part and its window apart from the page coming in.
    #[test]
    fn the_floor_keeps_a_part_and_its_window_beside_a_page_coming_in() {
        // A part shown, with a page coming in, and no room left.
        assert_eq!(floor_allowance((PAGE, PAGE, None), 0), Some(PAGE_READ));
        // Room left beside the page coming in: sized to it.
        assert_eq!(
            floor_allowance((PAGE, PAGE, None), 2 * PAGE + 5_000),
            Some(PAGE + 5_000)
        );
        // More than a part and its window held: not even the floor.
        assert_eq!(floor_allowance((PAGE_READ, 0, None), 0), None);
        // A whole read coming in is no page.
        assert_eq!(floor_allowance((0, 2 * PAGE, None), 0), None);
        // A first read under way keeps its allowance only within the floor.
        assert_eq!(floor_allowance((0, 0, Some(PAGE_READ)), 0), Some(PAGE_READ));
        assert_eq!(floor_allowance((0, 0, Some(PAGE_READ + 1)), 0), None);
    }

    /// SUB decision `a884e4c6`: the floor belongs to the latest request
    /// alone; any other read is fitted only within what is left.
    #[test]
    fn only_the_latest_request_is_fitted_to_the_floor() {
        let shown = (PAGE, 0, None);
        assert_eq!(kept_allowance(shown, (0, true)), Some(PAGE_READ));
        assert_eq!(kept_allowance(shown, (0, false)), None);
        let left = PAGE + MIN_WINDOW;
        assert_eq!(kept_allowance(shown, (left, false)), Some(left));
        assert_eq!(kept_allowance(shown, (left - 1, false)), None);
        // A first read under way keeps its allowance only within what is left.
        assert_eq!(
            kept_allowance((0, 0, Some(PAGE)), (PAGE, false)),
            Some(PAGE)
        );
        assert_eq!(kept_allowance((0, 0, Some(PAGE)), (PAGE - 1, false)), None);
    }

    /// SUB decision `a884e4c6`: a read replacing what is shown takes no
    /// more than the room newer requests leave it once it arrives, and
    /// when that does not hold what arrived, what is shown stays.
    #[test]
    fn a_replacement_arriving_is_granted_only_the_room_newer_requests_leave() {
        let granted = PAGE_READ;
        // Deimos's case: 85,000 bytes left beside a newer body.
        assert_eq!(swap_allowance(granted, PAGE, (85_000, false)), Some(85_000));
        assert_eq!(
            swap_allowance(granted, PAGE, (u64::MAX, false)),
            Some(granted)
        );
        assert_eq!(
            swap_allowance(granted, PAGE, (PAGE + MIN_WINDOW - 1, false)),
            None
        );
        // The latest request keeps its grant: the floor is its own.
        assert_eq!(swap_allowance(granted, PAGE, (0, true)), Some(granted));
    }
}
