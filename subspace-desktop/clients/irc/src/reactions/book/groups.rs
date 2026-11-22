//! One message's reactions: the latest operation per reaction and person.

use super::{
    MAX_REACTIONS_PER_MESSAGE, MAX_REACTORS_PER_REACTION, REMOVALS_FACTOR, ReactionSummary, Reactor,
};
use crate::model::{Casemapping, Reaction, ReactionChange};
use crate::reactions::agent_stop::is_agent_stop_reaction;

/// The latest operation one person made with one reaction.
#[derive(Debug)]
pub(super) struct Record {
    pub(super) reactor: Reactor,
    /// `reactor.nick` folded with the network's case mapping.
    pub(super) folded: String,
    pub(super) added: bool,
}

#[derive(Debug)]
pub(super) struct Group {
    reaction: String,
    pub(super) records: Vec<Record>,
}

impl Group {
    fn shown(&self) -> usize {
        self.records.iter().filter(|record| record.added).count()
    }
}

/// Applies `reaction` to its group; whether anything changed.
pub(super) fn record(groups: &mut Vec<Group>, reaction: &Reaction) -> bool {
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
    let changed = upsert(&mut groups[index], reaction);
    if groups[index].records.is_empty() {
        groups.remove(index);
    }
    changed
}

fn room_for_group(groups: &[Group], added: bool) -> bool {
    let shown = groups.iter().filter(|group| group.shown() > 0).count();
    let limit = MAX_REACTIONS_PER_MESSAGE;
    groups.len() < limit * REMOVALS_FACTOR && (!added || shown < limit)
}

/// Brings one person's record up to `reaction` unless it is older than
/// what is held; whether anything changed.
fn upsert(group: &mut Group, reaction: &Reaction) -> bool {
    let incoming = Record {
        reactor: Reactor {
            nick: reaction.nick.clone(),
            account: reaction.account.clone(),
            own: reaction.own,
            time: reaction.time,
        },
        folded: reaction.casemapping.fold(&reaction.nick),
        added: reaction.change == ReactionChange::Added,
    };
    let mapping = reaction.casemapping;
    match group
        .records
        .iter_mut()
        .find(|held| same(held, &incoming, mapping))
    {
        Some(held) => merge(held, incoming),
        None => insert(group, incoming),
    }
}

/// A later operation by someone already held. Two additions keep the
/// earlier time, so a person's place stays where they first reacted.
fn merge(held: &mut Record, incoming: Record) -> bool {
    let own = held.reactor.own || incoming.reactor.own;
    let changed = match (held.added, incoming.added) {
        (true, true) => {
            let time = held.reactor.time.min(incoming.reactor.time);
            let moved = time != held.reactor.time;
            held.reactor.time = time;
            moved
        }
        _ if incoming.reactor.time < held.reactor.time => false,
        (was, now) => {
            held.added = now;
            held.reactor.time = incoming.reactor.time;
            was != now
        }
    };
    let marked = own != held.reactor.own;
    held.reactor.own = own;
    changed || marked
}

/// A new person: shown if there is room, a removal remembered if there is
/// room for it, dropping the oldest remembered removal first.
fn insert(group: &mut Group, incoming: Record) -> bool {
    let limit = MAX_REACTORS_PER_REACTION;
    if incoming.added && group.shown() >= limit {
        return false;
    }
    if group.records.len() >= limit * REMOVALS_FACTOR {
        let oldest = group
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| !record.added)
            .min_by_key(|(_, record)| record.reactor.time)
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

/// The same person: both ours, the same account when both are known, or
/// the same nick when neither has an account.
fn same(one: &Record, other: &Record, mapping: Casemapping) -> bool {
    if one.reactor.own && other.reactor.own {
        return true;
    }
    match (&one.reactor.account, &other.reactor.account) {
        (Some(one), Some(other)) => mapping.fold(one) == mapping.fold(other),
        (None, None) => one.folded == other.folded,
        _ => false,
    }
}
