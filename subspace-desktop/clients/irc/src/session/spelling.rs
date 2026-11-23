//! One display spelling per channel or query, so `#Room` and `#room` (or
//! `[x]` and `{x}` under RFC 1459 case mapping) name the same buffer. The
//! first spelling seen wins and every event carries it.

use std::collections::BTreeMap;

use crate::model::{BufferKey, Event, Target, Update};

#[derive(Debug, Default)]
pub struct Spellings {
    /// Display spellings by folded name.
    by_key: BTreeMap<String, String>,
}

impl Spellings {
    /// The spelling recorded for `key`, recording `name` when it is new.
    pub fn canonical(&mut self, key: String, name: &str) -> String {
        self.by_key
            .entry(key)
            .or_insert_with(|| name.to_string())
            .clone()
    }

    /// Every recorded spelling, to carry over to the next connection.
    pub fn names(&self) -> Vec<String> {
        self.by_key.values().cloned().collect()
    }

    /// Records `names` again under `fold`, after the case mapping changed
    /// or on a new connection. The first spelling of a folded name wins.
    pub fn refold(&mut self, names: Vec<String>, fold: impl Fn(&str) -> String) {
        self.by_key.clear();
        for name in names {
            self.by_key.entry(fold(&name)).or_insert(name);
        }
    }
}

/// The name a buffer target carries: a channel or a nick.
pub(super) fn name_mut(target: &mut Target) -> Option<&mut String> {
    match target {
        Target::Channel(name) | Target::Query(name) => Some(name),
        Target::Server => None,
    }
}

/// Every buffer key an event names, including its history lines' keys.
pub(super) fn buffers_mut(event: &mut Event) -> Vec<&mut BufferKey> {
    match event {
        Event::Line(line) | Event::Replayed(line) => vec![&mut line.buffer],
        Event::History { buffer, lines, .. } | Event::Backfill { buffer, lines } => {
            std::iter::once(buffer)
                .chain(lines.iter_mut().map(|line| &mut line.buffer))
                .collect()
        }
        Event::AutojoinRetired(buffer)
        | Event::Joined { buffer }
        | Event::Left { buffer }
        | Event::Topic { buffer, .. }
        | Event::Members { buffer, .. }
        | Event::MemberJoined { buffer, .. }
        | Event::MemberLeft { buffer, .. }
        | Event::MemberChanged { buffer, .. }
        | Event::Typing { buffer, .. }
        | Event::Sent { buffer, .. }
        | Event::ReadMarker { buffer, .. } => vec![buffer],
        Event::Status { .. }
        | Event::NickStatus { .. }
        | Event::Capabilities { .. }
        | Event::OwnNick { .. }
        | Event::ChannelList { .. } => Vec::new(),
    }
}

/// Every buffer key an update names.
pub(super) fn update_buffers_mut(update: &mut Update) -> Vec<&mut BufferKey> {
    match update {
        Update::Event(event) => buffers_mut(event),
        Update::Reaction(reaction) => vec![&mut reaction.buffer],
        Update::ReactionRefused { buffer, .. } => vec![buffer],
        Update::ReactionSupport { .. } => Vec::new(),
    }
}
