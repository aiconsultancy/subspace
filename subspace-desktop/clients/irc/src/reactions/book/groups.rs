//! One message's reactions: the latest operation per reaction and person.

use super::{
    MAX_REACTIONS_PER_MESSAGE, MAX_REACTORS_PER_REACTION, REMOVALS_FACTOR, ReactionSummary, Reactor,
};
use chrono::{DateTime, Utc};

mod record;
pub(super) use record::Record;
use record::{incoming, merge, same};

use crate::model::{Casemapping, Reaction, ReactionChange};
use crate::reactions::agent_stop::is_agent_stop_reaction;

#[derive(Debug)]
pub(super) struct Group {
    reaction: String,
    pub(super) records: Vec<Record>,
}

impl Group {
    fn shown(&self) -> usize {
        self.records.iter().filter(|record| record.added).count()
    }

    /// The newest operation in the group, for dropping the oldest group.
    fn latest(&self) -> Option<DateTime<Utc>> {
        self.records.iter().map(|record| record.latest).max()
    }
}

/// Applies `reaction` to its group; whether anything changed. `stamp` is
/// the book's clock, newer than any record's.
pub(super) fn record(groups: &mut Vec<Group>, reaction: &Reaction, stamp: u64) -> bool {
    let added = reaction.change == ReactionChange::Added;
    let index = groups
        .iter()
        .position(|group| group.reaction == reaction.reaction);
    let index = match index {
        Some(index) => index,
        None if !room_for_group(groups, added) => return false,
        None => {
            groups.push(Group {
                reaction: reaction.reaction.clone(),
                records: Vec::new(),
            });
            groups.len() - 1
        }
    };
    let can_show = can_show(groups, index);
    let changed = upsert(
        &mut groups[index],
        incoming(reaction, stamp),
        can_show,
        reaction.casemapping,
    );
    if groups[index].records.is_empty() {
        groups.remove(index);
    }
    changed
}

/// Whether one more person may show in the group at `index`: below the
/// people limit, and the group already shows or another group fits.
fn can_show(groups: &[Group], index: usize) -> bool {
    let shown_groups = groups.iter().filter(|group| group.shown() > 0).count();
    let group = &groups[index];
    group.shown() < MAX_REACTORS_PER_REACTION
        && (group.shown() > 0 || shown_groups < MAX_REACTIONS_PER_MESSAGE)
}

/// Whether a new group fits. An addition that could not show is refused
/// before anything is dropped; otherwise, when every place is taken, the
/// group only remembering removals whose newest operation is oldest goes.
fn room_for_group(groups: &mut Vec<Group>, added: bool) -> bool {
    let limit = MAX_REACTIONS_PER_MESSAGE;
    let shown = groups.iter().filter(|group| group.shown() > 0).count();
    if added && shown >= limit {
        return false;
    }
    if groups.len() < limit * REMOVALS_FACTOR {
        return true;
    }
    let oldest = groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.shown() == 0)
        .min_by_key(|(_, group)| group.latest())
        .map(|(index, _)| index);
    oldest.map(|index| groups.remove(index)).is_some()
}

/// Brings one person's record up to `incoming` unless it is older than
/// what is held; whether anything changed. Of several records that match,
/// the one that most recently took the nick is that person.
fn upsert(group: &mut Group, incoming: Record, can_show: bool, mapping: Casemapping) -> bool {
    let held = group
        .records
        .iter_mut()
        .filter(|held| same(held, &incoming, mapping))
        .max_by_key(|held| held.assigned);
    match held {
        Some(held) => merge(held, incoming, can_show),
        None => insert(group, incoming, can_show),
    }
}

/// A new person: shown if there is room, a removal remembered if there is
/// room for it, dropping the oldest remembered removal (by when it was
/// removed) first.
fn insert(group: &mut Group, incoming: Record, can_show: bool) -> bool {
    if incoming.added && !can_show {
        return false;
    }
    if group.records.len() >= MAX_REACTORS_PER_REACTION * REMOVALS_FACTOR {
        let oldest = group
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| !record.added)
            .min_by_key(|(_, record)| record.latest)
            .map(|(index, _)| index);
        match oldest {
            Some(index) => group.records.remove(index),
            None => return false,
        };
    }
    group.records.push(incoming);
    true
}

/// Shown reactions, each with its people in time order, groups ordered by
/// their first person.
pub(super) fn summaries(groups: &[Group]) -> Vec<ReactionSummary> {
    let mut shown: Vec<ReactionSummary> = groups
        .iter()
        .map(|group| {
            let mut reactors: Vec<Reactor> = group
                .records
                .iter()
                .filter(|record| record.added)
                .map(|record| record.reactor.clone())
                .collect();
            reactors.sort_by_key(|reactor| reactor.time);
            ReactionSummary {
                reaction: group.reaction.clone(),
                reactors,
                stops_agent: is_agent_stop_reaction(&group.reaction),
            }
        })
        .filter(|summary| !summary.reactors.is_empty())
        .collect();
    shown.sort_by_key(|summary| summary.reactors.first().map(|reactor| reactor.time));
    shown
}
