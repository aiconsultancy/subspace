//! Which item bodies the view draws (SUB decisions `a17870f4` §5 and
//! `ed71a299`). What the open turns draw counts against the one budget
//! beside the inline content and the items read: a block its length, or
//! one part when it is long; an unknown item its pretty JSON; an item read
//! on demand the bytes it shows. Collapsed turns build no bodies, so they
//! draw nothing.
//!
//! Bodies are drawn only within the room the budget leaves. The bodies
//! the person asked for come first, the latest first; then the open
//! turns, the most recently opened first, and their items in order. An
//! item whose body would pass the room left shows its label, a caption
//! with its size saying it is not drawn to stay within the view's memory
//! budget, and **Show**. Nothing is cut: every body is one click away.
//! **Show** makes room, letting go of the least recently opened items read
//! first and then of the bodies shown earlier, the least recent first,
//! and draws the body asked for. Reading an item asks for its body too.
//!
//! The floor: inline content is the snapshot itself and is never let go.
//! When it alone fills the budget, the view holds no items read, one page
//! per part read in flight, and the one body last asked for.

use std::cmp::Reverse;
use std::collections::HashMap;

use gpui_kit::{AnyElement, Context, IntoElement as _, SharedString};
use subspace_client_fleet::activity::{ActivityEntry, ActivityTurn};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;

use super::parts::{caption, card};
use super::size::body_bytes;
use super::turn::is_open;
use super::{Held, Loaded, Selection};
use crate::Irc;

/// An item, by its turn and item IDs.
pub(super) type Key = (String, String);

/// What the open turns draw.
#[derive(Default)]
pub(super) struct Plan {
    /// The items not drawn, with the bytes their bodies would draw.
    pub(super) held_back: HashMap<Key, u64>,
    /// The bytes the bodies drawn hold.
    pub(super) drawn: u64,
}

/// Which bodies the open turns draw within the room left.
pub(super) fn plan(selection: &Selection) -> Plan {
    let bodies = bodies(selection);
    let room = super::load::room(selection).saturating_sub(super::load::held(selection));
    fit(&bodies, &selection.asked, room)
}

/// Every body the open turns would draw, the most recently opened turn
/// first, and its items in order.
fn bodies(selection: &Selection) -> Vec<(Key, u64)> {
    let Some(activity) = selection.join.activity() else {
        return Vec::new();
    };
    let mut open: Vec<&ActivityTurn> = activity
        .turns
        .iter()
        .rev()
        .filter(|turn| is_open(selection, &turn.summary))
        .collect();
    // Stable: turns open by default keep their order, newest first.
    open.sort_by_key(|turn| Reverse(selection.turn_opened.get(&turn.summary.id).copied()));
    open.into_iter()
        .flat_map(|turn| {
            turn.items.iter().map(|entry| {
                let key = (turn.summary.id.clone(), entry.base().id.clone());
                let bytes = drawn_bytes(entry, selection.loaded.get(&key));
                (key, bytes)
            })
        })
        .collect()
}

/// What an item's body draws: an item read on demand, what is shown of it.
fn drawn_bytes(entry: &ActivityEntry, loaded: Option<&Loaded>) -> u64 {
    match (entry, loaded.map(|loaded| &loaded.held)) {
        (ActivityEntry::Paged(_), Some(Held::Whole(read))) => body_bytes(read),
        (ActivityEntry::Paged(_), Some(Held::Parts(parts))) => parts.page.data.len() as u64,
        (ActivityEntry::Paged(_), _) => 0,
        (entry, _) => body_bytes(entry),
    }
}

/// Draws the bodies asked for, the latest first, then the rest in order,
/// each while it fits `room`. The body last asked for is drawn whatever
/// its size: the floor.
fn fit(bodies: &[(Key, u64)], asked: &HashMap<Key, u64>, room: u64) -> Plan {
    let mut order: Vec<&(Key, u64)> = bodies.iter().collect();
    // Stable: the rest keep their order after the bodies asked for.
    order.sort_by_key(|(key, _)| Reverse(asked.get(key).copied()));
    let latest = order.first().filter(|(key, _)| asked.contains_key(key));
    let mut plan = Plan::default();
    for (key, bytes) in order.iter().copied() {
        let floor = latest.is_some_and(|(latest, _)| latest == key);
        if floor || plan.drawn.saturating_add(*bytes) <= room {
            plan.drawn = plan.drawn.saturating_add(*bytes);
        } else {
            plan.held_back.insert(key.clone(), *bytes);
        }
    }
    plan
}

/// The bytes the bodies asked for draw, in open turns, `besides` one.
fn asked_bytes(selection: &Selection, besides: &Key) -> u64 {
    bodies(selection)
        .into_iter()
        .filter(|(key, _)| key != besides && selection.asked.contains_key(key))
        .map(|(_, bytes)| bytes)
        .sum()
}

/// The bytes the body of `key` draws, when its turn is open.
fn body_of(selection: &Selection, key: &Key) -> u64 {
    bodies(selection)
        .into_iter()
        .find(|(drawn, _)| drawn == key)
        .map_or(0, |(_, bytes)| bytes)
}

/// Asks for the body of `key` and makes room for it.
pub(super) fn ask(selection: &mut Selection, key: &Key) {
    selection.opened += 1;
    selection.asked.insert(key.clone(), selection.opened);
    make_room(selection, key);
}

/// Lets go of the least recently opened items read, then of the bodies
/// asked for earlier, the least recent first, other than `key`'s, until
/// its body fits the budget beside them, or nothing else is left.
fn make_room(selection: &mut Selection, key: &Key) {
    let wanted = body_of(selection, key);
    loop {
        let used = super::load::held(selection)
            .saturating_add(asked_bytes(selection, key))
            .saturating_add(wanted);
        if used <= super::load::room(selection) {
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
            continue;
        }
        let oldest_asked = selection
            .asked
            .iter()
            .filter(|(asked, _)| *asked != key)
            .min_by_key(|(_, opened)| **opened)
            .map(|(asked, _)| asked.clone());
        let Some(asked) = oldest_asked else {
            return;
        };
        selection.asked.remove(&asked);
    }
}

/// Shows the body of the item `key`, as **Show** asks.
pub(super) fn show(board: &mut Irc, key: &Key, cx: &mut Context<Irc>) {
    if let Some(selection) = board.inspector_activity.as_mut() {
        ask(selection, key);
        cx.notify();
    }
}

/// An item whose body is not drawn: its label, why, its size, and
/// **Show**.
pub(super) fn held_back(
    id: &str,
    (key, entry, bytes): (&Key, &ActivityEntry, u64),
    cx: &Context<Irc>,
) -> AnyElement {
    let key = key.clone();
    let show = action_button(
        SharedString::from(format!("{id}:show")),
        Intent::Neutral,
        Some(IconName::Eye),
        "Show",
    )
    .on_click(cx.listener(move |board, _, _, cx| self::show(board, &key, cx)))
    .into_any_element();
    card(
        id,
        &super::item::label(entry),
        vec![
            caption(
                &format!("{id}:held-back"),
                &format!("Not drawn, to stay within the view's memory budget ({bytes} bytes)."),
                cx,
            ),
            show,
        ],
        cx,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{Key, fit};

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

    /// The bodies asked for come first, the latest first, and the latest
    /// is drawn even past the room: the floor.
    #[test]
    fn the_body_last_asked_for_is_drawn_first_whatever_its_size() {
        let bodies = [(key("a"), 40), (key("b"), 50), (key("c"), 100)];
        let asked = HashMap::from([(key("b"), 1), (key("c"), 2)]);
        let plan = fit(&bodies, &asked, 60);
        assert_eq!(plan.drawn, 100);
        assert_eq!(
            plan.held_back,
            HashMap::from([(key("a"), 40), (key("b"), 50)])
        );
    }
}
