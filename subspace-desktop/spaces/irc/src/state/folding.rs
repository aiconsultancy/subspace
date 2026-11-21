//! Keeps each buffer's folded request progress and its requests' pins
//! current ([`crate::thread_status`]) after the events that can change
//! them, in proportion to what they change:
//!
//! - a live line is taken in by itself: a request is pinned or waits for
//!   an account, and progress folds or not, without reading the buffer
//!   again ([`arrived`]); dropping the oldest lines stops folding the
//!   notices of a request no longer held ([`make_room`]);
//! - history, backfill and lines restored from the store fold their buffer
//!   again from its held lines ([`Scope::Buffer`]);
//! - a member list change only pins requests still waiting for their
//!   agent's account, in the buffers that list speaks for, and folds a
//!   buffer again only when held progress answers one of them
//!   ([`Scope::Members`]). A pinned request never changes.

use std::collections::BTreeSet;

use subspace_client_irc::{BufferKey, Event};

use super::{Buffer, Entry, IrcState, LINE_CAP, counts};
use crate::thread_status::{self, Arrival};

/// What an event can change the folded progress of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Scope {
    /// One buffer's lines, all read again.
    Buffer(BufferKey),
    /// The requests waiting for an account in the buffers whose people the
    /// member list of `buffer` speaks for: for `nick` alone (ASCII lower
    /// case), or for every nick.
    Members {
        buffer: BufferKey,
        nick: Option<String>,
    },
}

/// What `event` may change: the buffer it adds older lines to, or, for a
/// member list change that can report an account, the waiting requests.
/// A live line is taken in as it is appended instead.
pub(super) fn scope(event: &Event) -> Option<Scope> {
    match event {
        Event::History { buffer, .. } | Event::Backfill { buffer, .. } => {
            Some(Scope::Buffer(buffer.clone()))
        }
        Event::Members { buffer, .. } => Some(Scope::Members {
            buffer: buffer.clone(),
            nick: None,
        }),
        Event::MemberJoined { buffer, member } | Event::MemberChanged { buffer, member, .. } => {
            member.account.as_ref().map(|_| Scope::Members {
                buffer: buffer.clone(),
                nick: Some(member.nick.to_ascii_lowercase()),
            })
        }
        _ => None,
    }
}

/// Folds again what `scope` covers.
pub(super) fn refold(state: &mut IrcState, scope: &Scope) {
    match scope {
        Scope::Buffer(key) => rebuild(state, key),
        Scope::Members { buffer, nick } => repin(state, buffer, nick.as_deref()),
    }
}

/// Pins each request buffer `key` holds that has no pin and whose agent's
/// account the server reports now, folds what is trusted, and takes folded
/// lines out of the unread count.
fn rebuild(state: &mut IrcState, key: &BufferKey) {
    let asked = thread_status::asked(state, key);
    let Some(buffer) = state.buffers.get_mut(key) else {
        return;
    };
    buffer.requests.rehold(&asked);
    let notices = thread_status::notices(buffer);
    buffer.requests.refold(&notices, &mut buffer.folded);
    counts::forget_folded(buffer);
}

/// Pins the waiting requests a member list change in `changed` reported an
/// account for, and folds again each buffer where held progress answers
/// one of them.
fn repin(state: &mut IrcState, changed: &BufferKey, nick: Option<&str>) {
    let found = thread_status::reported(state, changed, nick);
    let again: BTreeSet<BufferKey> = found
        .into_iter()
        .filter(|(key, nick, account)| {
            state
                .buffers
                .get_mut(key)
                .is_some_and(|held| held.requests.pin_waiting(nick, account))
        })
        .map(|(key, _, _)| key)
        .collect();
    again.iter().for_each(|key| rebuild(state, key));
}

/// Makes room for one more live line in buffer `key`, dropping its oldest
/// lines beyond the cap ([`dropped`]).
pub(super) fn make_room(state: &mut IrcState, key: &BufferKey) {
    let Some(buffer) = state.buffers.get_mut(key) else {
        return;
    };
    let cap = LINE_CAP + buffer.history.extra;
    while buffer.entries.len() >= cap
        && let Some(oldest) = buffer.entries.pop_front()
    {
        dropped(buffer, &oldest);
    }
}

/// Forgets line `entry`, no longer held: it is no longer unread or folded,
/// and when it was a request, its progress notices no longer fold, so they
/// show, and can be found, as ordinary notices.
fn dropped(buffer: &mut Buffer, entry: &Entry) {
    buffer.unread_entries.remove(&entry.seq);
    buffer.folded.remove(&entry.seq);
    let unfolded = entry
        .line
        .id
        .as_deref()
        .map(|id| buffer.requests.release(id))
        .unwrap_or_default();
    unfolded
        .iter()
        .for_each(|seq| _ = buffer.folded.remove(seq));
}

/// Takes in the live line buffer `key` now holds last, as `arrival` says,
/// and folds the buffer again when a request it pinned has held progress.
pub(super) fn arrived(state: &mut IrcState, key: &BufferKey, arrival: &Arrival) {
    let again = state
        .buffers
        .get_mut(key)
        .is_some_and(|buffer| buffer.requests.arrive(arrival, &mut buffer.folded));
    if again {
        rebuild(state, key);
    }
}
