//! Unread and highlight counts, kept against the buffer's read marker.
//! Folded request progress ([`Buffer::folded`]) never counts.

use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, Target};

use super::{Buffer, Entry, is_message};

/// Whether `entry` adds to the unread count of buffer `key`. A live line
/// counts unless the marker is already past it; a history line counts only
/// when it is newer than a known marker. Folded progress never counts.
fn unread(
    key: &BufferKey,
    entry: &Entry,
    marker: Option<DateTime<Utc>>,
    live: bool,
    folded: bool,
) -> bool {
    if folded || entry.line.own || !is_message(entry.line.kind) || key.target == Target::Server {
        return false;
    }
    match marker {
        Some(marker) => entry.line.time > marker,
        None => live,
    }
}

/// Counts a new live line, unless it is `folded` progress. True when it is
/// unread, the only lines that may notify.
pub(super) fn count(buffer: &mut Buffer, key: &BufferKey, entry: &Entry, folded: bool) -> bool {
    tally(buffer, key, entry, true, folded)
}

/// Counts an older line from history. Progress it turns out to be is taken
/// back out by [`forget_folded`] once the buffer is folded again.
pub(super) fn count_history(buffer: &mut Buffer, key: &BufferKey, entry: &Entry) {
    tally(buffer, key, entry, false, false);
}

fn tally(buffer: &mut Buffer, key: &BufferKey, entry: &Entry, live: bool, folded: bool) -> bool {
    let counted = unread(key, entry, buffer.read_marker, live, folded);
    if counted {
        buffer.unread += 1;
        buffer.highlights += usize::from(entry.highlight);
        buffer.unread_entries.insert(entry.seq);
    }
    counted
}

/// Counts again after the marker moved.
pub(super) fn recount(buffer: &mut Buffer) {
    let marker = buffer.read_marker;
    buffer.unread_entries.clear();
    buffer.highlights = 0;
    for entry in &buffer.entries {
        let folded = buffer.folded.contains(&entry.seq);
        if unread(&entry.line.buffer, entry, marker, true, folded) {
            buffer.unread_entries.insert(entry.seq);
            buffer.highlights += usize::from(entry.highlight);
        }
    }
    buffer.unread = buffer.unread_entries.len();
}

/// Takes folded progress back out of the unread count, such as history
/// counted before the member list could vouch for its sender.
pub(super) fn forget_folded(buffer: &mut Buffer) {
    let counted: Vec<(i64, bool)> = buffer
        .entries
        .iter()
        .filter(|entry| buffer.folded.contains(&entry.seq))
        .filter(|entry| buffer.unread_entries.contains(&entry.seq))
        .map(|entry| (entry.seq, entry.highlight))
        .collect();
    for (seq, highlight) in counted {
        buffer.unread_entries.remove(&seq);
        buffer.unread = buffer.unread.saturating_sub(1);
        buffer.highlights = buffer.highlights.saturating_sub(usize::from(highlight));
    }
    let folded = &buffer.folded;
    buffer.opened_unread.retain(|seq| !folded.contains(seq));
}
