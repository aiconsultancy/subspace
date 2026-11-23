//! One person's latest operation with one reaction, and how a newer or
//! older operation changes it.

use chrono::{DateTime, Utc};

use super::super::Reactor;
use crate::model::{Casemapping, Reaction, ReactionChange};

/// The latest operation one person made with one reaction. `reactor.time`
/// is when their current addition started, for display; `latest` is when
/// their newest operation happened, for telling newer from older.
#[derive(Debug, Clone)]
pub(in crate::reactions::book) struct Record {
    pub(in crate::reactions::book) reactor: Reactor,
    pub(super) latest: DateTime<Utc>,
    /// `latest` came from live traffic, which wins a tie with history.
    live: bool,
    /// When their newest known removal happened, so an older addition
    /// never moves their place back across it.
    removed: Option<DateTime<Utc>>,
    /// The book's stamp when this record last took its nick: of two
    /// accountless records with one nick, the newer holds it now.
    pub(in crate::reactions::book) assigned: u64,
    pub(in crate::reactions::book) added: bool,
}

pub(super) fn incoming(reaction: &Reaction, stamp: u64) -> Record {
    let added = reaction.change == ReactionChange::Added;
    Record {
        reactor: Reactor {
            nick: reaction.nick.clone(),
            account: reaction.account.clone(),
            own: reaction.own,
            time: reaction.time,
        },
        latest: reaction.time,
        live: !reaction.history,
        removed: (!added).then_some(reaction.time),
        assigned: stamp,
        added,
    }
}

/// Another operation by someone already held, and whether we are now
/// known to be among them. A removal, even an older one, is remembered
/// for [`backdate`].
pub(super) fn merge(held: &mut Record, incoming: Record, can_show: bool) -> bool {
    let own = held.reactor.own || incoming.reactor.own;
    let mut changed = false;
    if !incoming.added {
        held.removed = held.removed.max(Some(incoming.latest));
        changed = after_removal(held);
    }
    let changed = advance(held, incoming, can_show) || changed;
    let marked = own != held.reactor.own;
    held.reactor.own = own;
    changed || marked
}

/// One older than their newest changes nothing, except that an older
/// addition may move a shown person's place back to when they first
/// reacted, never across a known removal. A change that would show them
/// waits for room.
fn advance(held: &mut Record, incoming: Record, can_show: bool) -> bool {
    if held.added && incoming.added && incoming.latest < held.reactor.time {
        return backdate(held, incoming.latest);
    }
    let reveals = incoming.added && !held.added;
    if !newer(held, &incoming) || (reveals && !can_show) {
        return false;
    }
    take(held, &incoming)
}

/// Makes `incoming` the newest operation held; whether it shows or hides
/// the person.
fn take(held: &mut Record, incoming: &Record) -> bool {
    held.latest = incoming.latest;
    held.live = incoming.live;
    if incoming.added && !held.added {
        held.reactor.time = incoming.latest;
    }
    let was = held.added;
    held.added = incoming.added;
    was != incoming.added
}

/// A shown person's place never stays on or before a removal learned
/// late, as when a replayed addition backdated it first: it moves to their
/// newest addition, which came after that removal.
fn after_removal(held: &mut Record) -> bool {
    let stale = held.added
        && held
            .removed
            .is_some_and(|removed| held.reactor.time <= removed);
    if stale {
        held.reactor.time = held.latest;
    }
    stale
}

/// Moves a shown person's place back to `time`, unless that crosses
/// their newest known removal; whether it moved.
fn backdate(held: &mut Record, time: DateTime<Utc>) -> bool {
    let crosses = held.removed.is_some_and(|removed| time <= removed);
    if !crosses {
        held.reactor.time = time;
    }
    !crosses
}

/// Whether `incoming` is newer than what is held: a later time, or the
/// same time unless only the held operation was live.
fn newer(held: &Record, incoming: &Record) -> bool {
    incoming.latest > held.latest
        || (incoming.latest == held.latest && (incoming.live || !held.live))
}

/// The same person: both ours, the same account when both are known, or
/// the same nick under the network's current case mapping when neither
/// has an account.
pub(super) fn same(one: &Record, other: &Record, mapping: Casemapping) -> bool {
    if one.reactor.own && other.reactor.own {
        return true;
    }
    match (&one.reactor.account, &other.reactor.account) {
        (Some(one), Some(other)) => mapping.same(one, other),
        (None, None) => mapping.same(&one.reactor.nick, &other.reactor.nick),
        _ => false,
    }
}
