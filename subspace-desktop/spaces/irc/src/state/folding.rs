//! Keeps each buffer's folded request progress current
//! ([`crate::thread_status`]) after the events that can change it, in
//! proportion to what they change:
//!
//! - a live line is taken in by itself: progress folds or not, without
//!   reading the buffer again ([`arrived`]); dropping the oldest lines stops
//!   folding the notices of a request no longer held, and counts them as
//!   history would ([`make_room`]);
//! - history, backfill and lines restored from the store fold their buffer
//!   again from its held lines ([`Scope::Buffer`]).
//!
//! Member list changes change nothing: trust depends only on the request
//! and the notice.

use subspace_client_irc::{BufferKey, Event};

use super::{Buffer, Entry, IrcState, LINE_CAP, counts};
use crate::thread_status::{self, Arrival};

/// What an event can change the folded progress of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Scope {
    /// One buffer's lines, all read again.
    Buffer(BufferKey),
}

/// What `event` may change: the buffer it adds older lines to. A live line
/// is taken in as it is appended instead.
pub(super) fn scope(event: &Event) -> Option<Scope> {
    match event {
        Event::History { buffer, .. } | Event::Backfill { buffer, .. } => {
            Some(Scope::Buffer(buffer.clone()))
        }
        _ => None,
    }
}

/// Folds again what `scope` covers.
pub(super) fn refold(state: &mut IrcState, scope: &Scope) {
    match scope {
        Scope::Buffer(key) => rebuild(state, key),
    }
}

/// Folds what is trusted in buffer `key` from its held lines, and takes
/// folded lines out of the unread count.
fn rebuild(state: &mut IrcState, key: &BufferKey) {
    adopt(state, key);
    let Some(buffer) = state.buffers.get_mut(key) else {
        return;
    };
    let asked = thread_status::asked(buffer);
    buffer.requests.rehold(&asked);
    let notices = thread_status::notices(buffer);
    buffer.requests.refold(&notices, &mut buffer.folded);
    counts::forget_folded(buffer);
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
/// show, can be found and count against the read marker as history lines
/// do.
fn dropped(buffer: &mut Buffer, entry: &Entry) {
    buffer.unread_entries.remove(&entry.seq);
    buffer.folded.remove(&entry.seq);
    if let Some(request) = entry.line.reply_to.as_deref() {
        buffer.requests.forget_notice(request, entry.seq);
    }
    let unfolded = entry
        .line
        .id
        .as_deref()
        .map(|id| buffer.requests.release(id))
        .unwrap_or_default();
    unfolded
        .iter()
        .for_each(|seq| _ = buffer.folded.remove(seq));
    counts::unfolded(buffer, &unfolded);
}

/// Takes in the live line buffer `key` now holds last, as `arrival` says,
/// and folds the buffer again when a request it holds anew has held
/// progress.
pub(super) fn arrived(state: &mut IrcState, key: &BufferKey, arrival: &Arrival) {
    adopt(state, key);
    let again = state
        .buffers
        .get_mut(key)
        .is_some_and(|buffer| buffer.requests.arrive(arrival, &mut buffer.folded));
    if again {
        rebuild(state, key);
    }
}

/// Gives buffer `key` the endings the store kept for it while it held no
/// lines ([`IrcState::parked`]).
fn adopt(state: &mut IrcState, key: &BufferKey) {
    if let Some(buffer) = state.buffers.get_mut(key)
        && let Some(ended) = state.parked.remove(key)
    {
        buffer.requests.load(&ended);
    }
}
