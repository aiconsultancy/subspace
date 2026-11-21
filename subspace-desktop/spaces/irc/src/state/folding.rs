//! Keeps each buffer's folded request progress current
//! ([`crate::thread_status::trusted`]) after the events that can change it:
//! new lines, history, and member lists, whose accounts vouch for senders.
//! A line folds once the server vouches for it and stays folded while it is
//! held, so a member leaving never brings progress back as messages.

use std::collections::HashSet;

use subspace_client_irc::{BufferKey, Event, NetworkId};

use super::{Buffer, IrcState, counts};

/// The buffers an event can change the folded progress of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Scope {
    Buffer(BufferKey),
    /// Every buffer of a network: a channel's members also vouch for the
    /// network's private conversations.
    Network(NetworkId),
}

impl Scope {
    fn covers(&self, key: &BufferKey) -> bool {
        match self {
            Self::Buffer(buffer) => buffer == key,
            Self::Network(network) => &key.network == network,
        }
    }
}

/// What `event` may change: the buffer it adds lines to, or, for a member
/// list change, its network.
pub(super) fn scope(event: &Event) -> Option<Scope> {
    match event {
        Event::Line(line) | Event::Replayed(line) => Some(Scope::Buffer(line.buffer.clone())),
        Event::History { buffer, .. } | Event::Backfill { buffer, .. } => {
            Some(Scope::Buffer(buffer.clone()))
        }
        Event::Members { buffer, .. }
        | Event::MemberJoined { buffer, .. }
        | Event::MemberChanged { buffer, .. } => Some(Scope::Network(buffer.network.clone())),
        _ => None,
    }
}

/// Folds what is trusted now in every buffer `scope` covers, and takes
/// folded lines out of their unread counts.
pub(super) fn refold(state: &mut IrcState, scope: &Scope) {
    let shared: &IrcState = state;
    let found: Vec<(BufferKey, HashSet<i64>)> = shared
        .buffers
        .keys()
        .filter(|key| scope.covers(key))
        .map(|key| (key.clone(), crate::thread_status::trusted(shared, key)))
        .collect();
    found.into_iter().for_each(|(key, trusted)| {
        if let Some(buffer) = state.buffers.get_mut(&key) {
            keep(buffer, trusted);
        }
    });
}

/// Adds `trusted` to what `buffer` folded before, while those lines are
/// held.
fn keep(buffer: &mut Buffer, trusted: HashSet<i64>) {
    let held: HashSet<i64> = buffer.entries.iter().map(|entry| entry.seq).collect();
    buffer.folded.retain(|seq| held.contains(seq));
    buffer.folded.extend(trusted);
    counts::forget_folded(buffer);
}
