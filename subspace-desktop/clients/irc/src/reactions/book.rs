//! A bounded, pure aggregation of reactions per message for a UI to read.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::validate_reaction;
use crate::model::{BufferKey, Casemapping, Event, NetworkId, Reaction};

/// Messages with reactions (or remembered removals) kept; the least
/// recently changed goes first.
pub const MAX_MESSAGES: usize = 2048;
/// Distinct reactions shown per message; later new ones are ignored.
pub const MAX_REACTIONS_PER_MESSAGE: usize = 20;
/// People shown per reaction; later ones are ignored.
pub const MAX_REACTORS_PER_REACTION: usize = 100;
/// Each limit again for remembered removals.
const REMOVALS_FACTOR: usize = 2;

mod groups;
use groups::{Group, record, summaries};

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

#[derive(Debug, Default)]
struct Entry {
    touched: u64,
    groups: Vec<Group>,
}

/// Reactions per message (`msgid` within one buffer), each in first-reacted
/// order. It keeps the latest operation per message, reaction and person,
/// so an older replay never overwrites a newer change: a history page that
/// still holds an addition later removed leaves it removed. Reactions for
/// messages not loaded yet are kept too, so a later history page finds
/// them. Bounded by [`MAX_MESSAGES`], [`MAX_REACTIONS_PER_MESSAGE`] and
/// [`MAX_REACTORS_PER_REACTION`].
///
/// Feed it every [`crate::Update::Reaction`] with [`ReactionBook::apply`]
/// and every event with [`ReactionBook::observe`], so nick changes follow.
/// People are the same when both are ours, or both have the same account,
/// or neither has an account and their nicks match under the network's
/// case mapping.
#[derive(Debug, Default)]
pub struct ReactionBook {
    messages: BTreeMap<Key, Entry>,
    mappings: BTreeMap<NetworkId, Casemapping>,
    clock: u64,
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
        evict_for(&mut self.messages, &key);
        let entry = self.messages.entry(key.clone()).or_default();
        if record(&mut entry.groups, reaction) {
            self.clock += 1;
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

    /// Whether we reacted to the message with `reaction`.
    pub fn reacted(&self, buffer: &BufferKey, msgid: &str, reaction: &str) -> bool {
        self.reactions(buffer, msgid)
            .iter()
            .any(|summary| summary.reaction == reaction && summary.mine())
    }

    /// How many messages hold reactions or remembered removals.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Follows nick changes ([`Event::MemberChanged`] with a new nick).
    pub fn observe(&mut self, event: &Event) {
        if let Event::MemberChanged {
            buffer,
            old_nick,
            member,
        } = event
            && *old_nick != member.nick
        {
            let account = member.account.as_deref();
            self.rename(&buffer.network, old_nick, &member.nick, account);
        }
    }

    /// Someone on `network` with `account`, if known, is now known as
    /// `new`. Entries under `old` with another known account keep their
    /// nick: they were someone else.
    pub fn rename(&mut self, network: &NetworkId, old: &str, new: &str, account: Option<&str>) {
        let mapping = self.mapping(network);
        let old = mapping.fold(old);
        let account = account.map(|account| mapping.fold(account));
        let records = self
            .messages
            .iter_mut()
            .filter(|((buffer, _), _)| buffer.network == *network)
            .flat_map(|(_, entry)| entry.groups.iter_mut())
            .flat_map(|group| group.records.iter_mut());
        for record in records {
            let theirs = record
                .reactor
                .account
                .as_deref()
                .map(|held| mapping.fold(held));
            if record.folded == old && (theirs.is_none() || theirs == account) {
                new.clone_into(&mut record.reactor.nick);
                record.folded = mapping.fold(new);
            }
        }
    }

    fn mapping(&self, network: &NetworkId) -> Casemapping {
        self.mappings
            .get(network)
            .copied()
            .unwrap_or(Casemapping::Rfc1459)
    }
}

/// Makes room for a new `key` by dropping the least recently changed
/// message.
fn evict_for(messages: &mut BTreeMap<Key, Entry>, key: &Key) {
    if messages.len() < MAX_MESSAGES || messages.contains_key(key) {
        return;
    }
    let oldest = messages
        .iter()
        .min_by_key(|(_, entry)| entry.touched)
        .map(|(key, _)| key.clone());
    if let Some(oldest) = oldest {
        messages.remove(&oldest);
    }
}

#[cfg(test)]
mod tests;
