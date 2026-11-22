//! A bounded, pure aggregation of reactions per message for a UI to read.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::agent_stop::is_agent_stop_reaction;
use super::validate_reaction;
use crate::model::{Event, NetworkId, Reaction, ReactionChange};
use crate::session::Casemapping;

/// Messages with reactions kept; the least recently changed goes first.
pub const MAX_MESSAGES: usize = 2048;
/// Distinct reactions kept per message; later new ones are ignored.
pub const MAX_REACTIONS_PER_MESSAGE: usize = 20;
/// People kept per reaction; later ones are ignored.
pub const MAX_REACTORS_PER_REACTION: usize = 100;

/// Someone who reacted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reactor {
    pub nick: String,
    pub account: Option<String>,
    /// We reacted.
    pub own: bool,
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

    fn first(&self) -> Option<DateTime<Utc>> {
        self.reactors.first().map(|reactor| reactor.time)
    }
}

type Key = (NetworkId, String);

#[derive(Debug, Default)]
struct Entry {
    touched: u64,
    reactions: Vec<ReactionSummary>,
}

/// Reactions per message (`msgid`, per network), each in first-reacted
/// order. Reactions for messages not loaded yet are kept too, so a later
/// history page finds them. Bounded by [`MAX_MESSAGES`],
/// [`MAX_REACTIONS_PER_MESSAGE`] and [`MAX_REACTORS_PER_REACTION`].
///
/// Feed it every [`crate::Update::Reaction`] with [`ReactionBook::apply`]
/// and every event with [`ReactionBook::observe`], so nick changes follow.
/// The same person is matched by account when both sides have one, else by
/// nick, ignoring case.
#[derive(Debug, Default)]
pub struct ReactionBook {
    messages: BTreeMap<Key, Entry>,
    clock: u64,
}

impl ReactionBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or removes one reaction. Invalid reactions are ignored, as are
    /// duplicates and removals of reactions not held.
    pub fn apply(&mut self, reaction: &Reaction) {
        if validate_reaction(&reaction.reaction).is_err() || reaction.msgid.is_empty() {
            return;
        }
        let key = (reaction.buffer.network.clone(), reaction.msgid.clone());
        match reaction.change {
            ReactionChange::Added => self.add(key, reaction),
            ReactionChange::Removed => self.remove(&key, reaction),
        }
    }

    /// The reactions on one message, in first-reacted order.
    pub fn reactions(&self, network: &NetworkId, msgid: &str) -> &[ReactionSummary] {
        self.messages
            .get(&(network.clone(), msgid.to_owned()))
            .map_or(&[], |entry| entry.reactions.as_slice())
    }

    /// Whether we reacted to the message with `reaction`.
    pub fn reacted(&self, network: &NetworkId, msgid: &str, reaction: &str) -> bool {
        self.reactions(network, msgid)
            .iter()
            .any(|summary| summary.reaction == reaction && summary.mine())
    }

    /// How many messages hold reactions.
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
            self.rename(&buffer.network, old_nick, &member.nick);
        }
    }

    /// Someone on `network` is now known as `new`.
    pub fn rename(&mut self, network: &NetworkId, old: &str, new: &str) {
        let old = fold(old);
        let held = self
            .messages
            .iter_mut()
            .filter(|((id, _), _)| id == network)
            .flat_map(|(_, entry)| entry.reactions.iter_mut());
        for summary in held {
            summary
                .reactors
                .iter_mut()
                .filter(|reactor| fold(&reactor.nick) == old)
                .for_each(|reactor| new.clone_into(&mut reactor.nick));
            dedupe(&mut summary.reactors);
        }
    }

    fn add(&mut self, key: Key, reaction: &Reaction) {
        self.clock += 1;
        evict_for(&mut self.messages, &key);
        let entry = self.messages.entry(key.clone()).or_default();
        entry.touched = self.clock;
        add_reactor(&mut entry.reactions, reaction);
        if entry.reactions.is_empty() {
            self.messages.remove(&key);
        }
    }

    fn remove(&mut self, key: &Key, reaction: &Reaction) {
        let Some(entry) = self.messages.get_mut(key) else {
            return;
        };
        let person = reactor(reaction);
        entry.reactions.iter_mut().for_each(|summary| {
            if summary.reaction == reaction.reaction {
                summary.reactors.retain(|held| !same(held, &person));
            }
        });
        entry
            .reactions
            .retain(|summary| !summary.reactors.is_empty());
        if entry.reactions.is_empty() {
            self.messages.remove(key);
        }
    }
}

/// Makes room for `key` by dropping the least recently changed message.
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

fn add_reactor(reactions: &mut Vec<ReactionSummary>, reaction: &Reaction) {
    let person = reactor(reaction);
    let index = match reactions
        .iter()
        .position(|summary| summary.reaction == reaction.reaction)
    {
        Some(index) => index,
        None if reactions.len() >= MAX_REACTIONS_PER_MESSAGE => return,
        None => {
            reactions.push(ReactionSummary {
                reaction: reaction.reaction.clone(),
                reactors: Vec::new(),
                stops_agent: is_agent_stop_reaction(&reaction.reaction),
            });
            reactions.len() - 1
        }
    };
    insert_reactor(&mut reactions[index].reactors, person);
    reactions.sort_by_key(ReactionSummary::first);
}

/// Adds `person` in time order unless they already reacted; a held entry
/// learns that it is ours.
fn insert_reactor(reactors: &mut Vec<Reactor>, person: Reactor) {
    if let Some(held) = reactors.iter_mut().find(|held| same(held, &person)) {
        held.own |= person.own;
        return;
    }
    if reactors.len() < MAX_REACTORS_PER_REACTION {
        let at = reactors.partition_point(|held| held.time <= person.time);
        reactors.insert(at, person);
    }
}

/// Keeps the earliest entry for each person.
fn dedupe(reactors: &mut Vec<Reactor>) {
    let mut kept: Vec<Reactor> = Vec::with_capacity(reactors.len());
    for reactor in reactors.drain(..) {
        match kept.iter_mut().find(|held| same(held, &reactor)) {
            Some(held) => held.own |= reactor.own,
            None => kept.push(reactor),
        }
    }
    *reactors = kept;
}

fn reactor(reaction: &Reaction) -> Reactor {
    Reactor {
        nick: reaction.nick.clone(),
        account: reaction.account.clone(),
        own: reaction.own,
        time: reaction.time,
    }
}

/// The same person: equal accounts when both are known, else equal nicks.
fn same(one: &Reactor, other: &Reactor) -> bool {
    match (&one.account, &other.account) {
        (Some(one), Some(other)) => one.eq_ignore_ascii_case(other),
        _ => fold(&one.nick) == fold(&other.nick),
    }
}

fn fold(nick: &str) -> String {
    Casemapping::Rfc1459.fold(nick)
}

#[cfg(test)]
mod tests;
