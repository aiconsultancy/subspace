//! A bounded, pure aggregation of reactions per message for a UI to read.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chrono::{DateTime, Utc};

use super::validate_reaction;
use crate::model::{BufferKey, Casemapping, Event, Line, NetworkId, Reaction};

/// Loaded messages with reactions (or remembered removals) kept; the
/// least recently changed goes first.
pub const MAX_MESSAGES: usize = 2048;
/// Messages not seen loaded yet whose reactions are kept, in all, apart
/// from [`MAX_MESSAGES`]: reactions naming unknown messages never displace
/// loaded ones.
pub const MAX_UNSEEN_MESSAGES: usize = 256;
/// Of those, at most this many per conversation.
pub const MAX_UNSEEN_PER_BUFFER: usize = 64;
/// Distinct reactions shown per message; later new ones are ignored.
pub const MAX_REACTIONS_PER_MESSAGE: usize = 20;
/// People shown per reaction; later ones are ignored.
pub const MAX_REACTORS_PER_REACTION: usize = 100;
/// Each limit again for remembered removals.
const REMOVALS_FACTOR: usize = 2;
/// Message IDs remembered as seen loaded.
const MAX_SEEN_IDS: usize = 8192;
/// Nicks remembered with when a rename last took them.
const MAX_HOLDERS: usize = 1024;

mod groups;
use groups::{Group, record, shows, summaries};

/// Someone who reacted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reactor {
    pub nick: String,
    pub account: Option<String>,
    /// We reacted.
    pub own: bool,
    /// When they reacted.
    pub time: DateTime<Utc>,
}

/// One reaction on a message and who reacted with it, earliest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionSummary {
    pub reaction: String,
    pub reactors: Vec<Reactor>,
    /// One of Stargate's default stop reactions, so a UI can mark it; see
    /// [`super::agent_stop`] for when it actually stops an agent.
    pub stops_agent: bool,
}

impl ReactionSummary {
    pub fn count(&self) -> usize {
        self.reactors.len()
    }

    /// Whether we are among those who reacted.
    pub fn mine(&self) -> bool {
        self.reactors.iter().any(|reactor| reactor.own)
    }
}

type Key = (BufferKey, String);

#[derive(Debug, Clone, Default)]
struct Entry {
    touched: u64,
    /// The message has been seen loaded, so it counts against
    /// [`MAX_MESSAGES`] rather than the unseen budget.
    seen: bool,
    groups: Vec<Group>,
}

/// Reactions per message (`msgid` within one buffer), each in first-reacted
/// order. It keeps the latest operation per message, reaction and person,
/// so an older replay never overwrites a newer change: a history page that
/// still holds an addition later removed leaves it removed. Reactions for
/// messages not loaded yet are kept too, so a later history page finds
/// them, within their own budget ([`MAX_UNSEEN_MESSAGES`],
/// [`MAX_UNSEEN_PER_BUFFER`]) until the message is seen. Bounded by
/// [`MAX_MESSAGES`], [`MAX_REACTIONS_PER_MESSAGE`] and
/// [`MAX_REACTORS_PER_REACTION`]; a remembered removal never makes room to
/// show more than those limits.
///
/// Feed it every [`crate::Update::Reaction`] with [`ReactionBook::apply`]
/// and every event with [`ReactionBook::observe`], so nick changes follow
/// and loaded messages are known. Of two operations at the same time, a
/// live one wins over one from history.
/// People are the same when both are ours, or both have the same account,
/// or neither has an account and their nicks match under the network's
/// case mapping.
#[derive(Debug, Clone, Default)]
pub struct ReactionBook {
    messages: BTreeMap<Key, Entry>,
    mappings: BTreeMap<NetworkId, Casemapping>,
    /// When each nick, folded, was last taken by a rename on its network:
    /// accountless records under it from before then were someone else's.
    holders: BTreeMap<(NetworkId, String), u64>,
    seen: Seen,
    clock: u64,
}

/// Message IDs seen loaded, oldest first, bounded by [`MAX_SEEN_IDS`].
#[derive(Debug, Clone, Default)]
struct Seen {
    ids: BTreeSet<Key>,
    order: VecDeque<Key>,
}

impl Seen {
    fn insert(&mut self, key: Key) {
        if !self.ids.insert(key.clone()) {
            return;
        }
        self.order.push_back(key);
        if self.order.len() > MAX_SEEN_IDS
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
    }

    fn forget(&mut self, buffer: &BufferKey) {
        self.ids.retain(|(held, _)| held != buffer);
        self.order.retain(|(held, _)| held != buffer);
    }
}

impl ReactionBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one added or removed reaction. Invalid reactions are
    /// ignored, as are operations older than the one held for that person.
    pub fn apply(&mut self, reaction: &Reaction) {
        if validate_reaction(&reaction.reaction).is_err() || reaction.msgid.is_empty() {
            return;
        }
        self.mappings
            .insert(reaction.buffer.network.clone(), reaction.casemapping);
        let key = (reaction.buffer.clone(), reaction.msgid.clone());
        let fresh = !self.messages.contains_key(&key);
        let seen = self.seen.ids.contains(&key);
        if fresh {
            evict_for(&mut self.messages, &key, seen);
        }
        self.clock += 1;
        let entry = self.messages.entry(key.clone()).or_default();
        entry.seen |= seen;
        if record(&mut entry.groups, reaction, self.clock) {
            entry.touched = self.clock;
        }
        if fresh && entry.groups.is_empty() {
            self.messages.remove(&key);
        }
    }

    /// The reactions on one message, in first-reacted order.
    pub fn reactions(&self, buffer: &BufferKey, msgid: &str) -> Vec<ReactionSummary> {
        self.messages
            .get(&(buffer.clone(), msgid.to_owned()))
            .map(|entry| summaries(&entry.groups))
            .unwrap_or_default()
    }

    /// The messages in `buffer` that show reactions, by message ID. Reads
    /// only `buffer`'s messages, and builds no summaries to find them.
    pub fn reacted_messages(&self, buffer: &BufferKey) -> Vec<&str> {
        self.messages
            .range((buffer.clone(), String::new())..)
            .take_while(|((held, _), _)| held == buffer)
            .filter(|(_, entry)| shows(&entry.groups))
            .map(|((_, msgid), _)| msgid.as_str())
            .collect()
    }

    /// Whether the message `msgid` in `buffer` is known to be loaded.
    pub fn is_seen(&self, buffer: &BufferKey, msgid: &str) -> bool {
        self.seen.ids.contains(&(buffer.clone(), msgid.to_owned()))
    }

    /// Whether we reacted to the message with `reaction`.
    pub fn reacted(&self, buffer: &BufferKey, msgid: &str, reaction: &str) -> bool {
        self.reactions(buffer, msgid)
            .iter()
            .any(|summary| summary.reaction == reaction && summary.mine())
    }

    /// The message `msgid` in `buffer` is loaded: its reactions now count
    /// against [`MAX_MESSAGES`], not the unseen budget. [`Self::observe`]
    /// calls it for every line with a `msgid`; an owner that loads lines
    /// another way, such as from its own store, calls it itself.
    pub fn mark_seen(&mut self, buffer: &BufferKey, msgid: &str) {
        let key = (buffer.clone(), msgid.to_owned());
        self.seen.insert(key.clone());
        if self.messages.get(&key).is_none_or(|entry| entry.seen) {
            return;
        }
        evict_for(&mut self.messages, &key, true);
        if let Some(entry) = self.messages.get_mut(&key) {
            entry.seen = true;
        }
    }

    /// Drops everything held for a conversation, such as when it is
    /// removed.
    pub fn forget(&mut self, buffer: &BufferKey) {
        self.messages.retain(|(held, _), _| held != buffer);
        self.seen.forget(buffer);
    }

    /// How many messages hold reactions or remembered removals.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Follows nick changes ([`Event::MemberChanged`] with a new nick)
    /// and marks the messages of lines, live or loaded, as seen.
    pub fn observe(&mut self, event: &Event) {
        match event {
            Event::MemberChanged {
                buffer,
                old_nick,
                member,
            } if *old_nick != member.nick => {
                let account = member.account.as_deref();
                self.rename(&buffer.network, old_nick, &member.nick, account);
            }
            Event::Line(line) | Event::Replayed(line) => self.see_lines(std::slice::from_ref(line)),
            Event::History { lines, .. } | Event::Backfill { lines, .. } => self.see_lines(lines),
            _ => {}
        }
    }

    fn see_lines(&mut self, lines: &[Line]) {
        for line in lines {
            if let Some(id) = &line.id {
                self.mark_seen(&line.buffer, id);
            }
        }
    }

    /// Someone on `network` with `account`, if known, is now known as
    /// `new`. Entries under `old` with another known account keep their
    /// nick: they were someone else. So do accountless entries made under
    /// `old` before its current holder took it by a rename: they belonged
    /// to an earlier holder of the nick.
    pub fn rename(&mut self, network: &NetworkId, old: &str, new: &str, account: Option<&str>) {
        let mapping = self.mapping(network);
        let since = self.held_since(network, old, mapping);
        self.clock += 1;
        let stamp = self.clock;
        let records = self
            .messages
            .iter_mut()
            .filter(|((buffer, _), _)| buffer.network == *network)
            .flat_map(|(_, entry)| entry.groups.iter_mut())
            .flat_map(|group| group.records.iter_mut());
        for record in records {
            if holds(record, (old, account), since, mapping) {
                new.clone_into(&mut record.reactor.nick);
                record.assigned = stamp;
            }
        }
        self.took(network, new, stamp, mapping);
    }

    /// When `nick` was last taken by a rename on `network`; 0 if never.
    fn held_since(&self, network: &NetworkId, nick: &str, mapping: Casemapping) -> u64 {
        self.holders
            .get(&(network.clone(), mapping.fold(nick)))
            .copied()
            .unwrap_or_default()
    }

    /// `nick` was taken by a rename at `stamp`. Keeps the newest
    /// [`MAX_HOLDERS`] nicks.
    fn took(&mut self, network: &NetworkId, nick: &str, stamp: u64, mapping: Casemapping) {
        self.holders
            .insert((network.clone(), mapping.fold(nick)), stamp);
        if self.holders.len() > MAX_HOLDERS
            && let Some(oldest) = self
                .holders
                .iter()
                .min_by_key(|(_, stamp)| **stamp)
                .map(|(key, _)| key.clone())
        {
            self.holders.remove(&oldest);
        }
    }

    fn mapping(&self, network: &NetworkId) -> Casemapping {
        self.mappings
            .get(network)
            .copied()
            .unwrap_or(Casemapping::Rfc1459)
    }
}

/// Whether `record` belongs to the person renamed from `old` with
/// `account`: under that nick, with that account or none, and, without an
/// account, made since the nick's current holder took it (`since`).
fn holds(
    record: &groups::Record,
    (old, account): (&str, Option<&str>),
    since: u64,
    mapping: Casemapping,
) -> bool {
    let reactor = &record.reactor;
    if !mapping.same(&reactor.nick, old) {
        return false;
    }
    match (reactor.account.as_deref(), account) {
        (None, _) => record.assigned >= since,
        (Some(held), Some(account)) => mapping.same(held, account),
        (Some(_), None) => false,
    }
}

/// Makes room for `key` among loaded messages (`seen`) or among those not
/// seen yet, dropping the least recently changed message of the same kind:
/// in the same conversation when its unseen quota is full.
fn evict_for(messages: &mut BTreeMap<Key, Entry>, key: &Key, seen: bool) {
    let others = || {
        messages
            .iter()
            .filter(move |(held, entry)| entry.seen == seen && *held != key)
    };
    let in_buffer = |held: &Key| held.0 == key.0;
    let full_buffer =
        !seen && others().filter(|(held, _)| in_buffer(held)).count() >= MAX_UNSEEN_PER_BUFFER;
    let limit = if seen {
        MAX_MESSAGES
    } else {
        MAX_UNSEEN_MESSAGES
    };
    if !full_buffer && others().count() < limit {
        return;
    }
    let oldest = others()
        .filter(|(held, _)| !full_buffer || in_buffer(held))
        .min_by_key(|(_, entry)| entry.touched)
        .map(|(held, _)| held.clone());
    if let Some(oldest) = oldest {
        messages.remove(&oldest);
    }
}

#[cfg(test)]
mod tests;
