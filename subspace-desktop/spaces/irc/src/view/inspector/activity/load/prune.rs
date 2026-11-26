//! Keeping what is held within the snapshot it belongs to: a snapshot of
//! a new generation forgets every body of the old one (SUB decision
//! `33ccecf6` item 3); otherwise the items read that the snapshot no
//! longer names as too large to send inline go, their charge with them
//! (SUB decision `4311830a` item 3), and the rest stays within the
//! budget, the latest request kept at the floor (SUB decisions `8357ec55`
//! §2 and `4311830a` item 1).

use std::collections::HashSet;

use subspace_client_fleet::activity::{ActivityEntry, ActivityTurn};

use super::super::Selection;
use super::super::draw::{Key, Plan, replan};
use super::super::ledger::latest;
use super::super::size::inline_bytes;
use super::make_room;

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

/// The items `turns` name as too large to send inline: the only ones an
/// item read may stand for.
fn paged_keys(turns: &[ActivityTurn]) -> HashSet<Key> {
    turns
        .iter()
        .flat_map(|turn| {
            turn.items
                .iter()
                .filter(|entry| matches!(entry, ActivityEntry::Paged(_)))
                .map(|entry| (turn.summary.id.clone(), entry.base().id.clone()))
        })
        .collect()
}

/// Counts the snapshot's inline content, forgets the bodies of an earlier
/// generation, lets go of the items read that it no longer names as too
/// large to send inline, of the requests and turns no longer in it and of
/// the parts shown of their texts, and keeps the rest within the budget,
/// the latest request kept at the floor.
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
    // An item sent inline now draws as it is, planned like any other: its
    // read stands for nothing and goes, its charge with it.
    let paged = paged_keys(&activity.turns);
    selection.loaded.retain(|key, _| paged.contains(key));
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
    let floor = latest(selection).cloned();
    make_room(selection, 0, floor.as_ref());
    replan(selection);
}
