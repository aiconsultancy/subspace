//! Keeping what is held within the snapshot it belongs to: a snapshot of
//! a new generation forgets every body of the old one (SUB decision
//! `33ccecf6` item 3); otherwise the items whose turn went go, and the
//! rest stays within the budget, never letting go of the read the floor
//! keeps (SUB decision `8357ec55` §2).

use std::collections::HashSet;

use super::super::Selection;
use super::super::draw::{Key, Plan, replan};
use super::super::size::inline_bytes;
use super::{FLOOR_READ, make_room};

/// The read the floor keeps through a new snapshot (SUB decision
/// `8357ec55` §2): the item last asked for, while it holds no more than a
/// part, its window and the page coming in.
fn floor_read(selection: &Selection) -> Option<Key> {
    let (key, _) = selection.asked.iter().max_by_key(|(_, asked)| **asked)?;
    let loaded = selection.loaded.get(key)?;
    (loaded.bytes <= FLOOR_READ).then(|| key.clone())
}

/// Forgets every body of a snapshot of an earlier generation (SUB
/// decision `33ccecf6` item 3): what was measured, prepared and drawn in
/// parts of it, the parts shown of its texts, and the items read for it,
/// their reads cancelled. A `seq` is compared only within a generation.
fn forget_generation(selection: &mut Selection, generation: String) {
    selection.generation = generation;
    selection.epoch += 1;
    selection.loaded.clear();
    selection.parts.clear();
    selection.measures.clear();
    selection.prepared.get_mut().clear();
    selection.plan = Plan::default();
}

/// Counts the snapshot's inline content, forgets the bodies of an earlier
/// generation, lets go of the items whose turn is no longer in it, and of
/// the parts shown of their texts, and keeps the rest within the budget,
/// never letting go of the read the floor keeps.
pub(in super::super) fn prune(selection: &mut Selection) {
    let Some(generation) = selection
        .join
        .activity()
        .map(|activity| activity.cursor.generation.clone())
    else {
        return;
    };
    if generation != selection.generation {
        forget_generation(selection, generation);
    }
    let Some(activity) = selection.join.activity() else {
        return;
    };
    let inline = inline_bytes(activity);
    let turns: HashSet<&str> = activity
        .turns
        .iter()
        .map(|turn| turn.summary.id.as_str())
        .collect();
    selection
        .loaded
        .retain(|(turn, _), _| turns.contains(turn.as_str()));
    selection
        .asked
        .retain(|(turn, _), _| turns.contains(turn.as_str()));
    selection
        .turn_opened
        .retain(|turn, _| turns.contains(turn.as_str()));
    // By the whole `irc-activity:{turn}:` prefix: a turn ID may hold `:`.
    let prefixes: Vec<String> = turns
        .iter()
        .map(|turn| format!("irc-activity:{turn}:"))
        .collect();
    selection.parts.retain(|block, _| {
        !block.starts_with("irc-activity:")
            || prefixes.iter().any(|prefix| block.starts_with(prefix))
    });
    selection.inline = inline;
    let floor = floor_read(selection);
    make_room(selection, 0, floor.as_ref());
    replan(selection);
}
