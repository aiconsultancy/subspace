//! Unread and highlight counts, kept against the buffer's read marker.

use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, Target};

use super::{Buffer, Entry, is_message};

/// Whether `entry` adds to the unread count of buffer `key`. A live line
/// counts unless the marker is already past it; a history line counts only
/// when it is newer than a known marker.
fn unread(key: &BufferKey, entry: &Entry, marker: Option<DateTime<Utc>>, live: bool) -> bool {
    if entry.line.own || !is_message(entry.line.kind) || key.target == Target::Server {
        return false;
    }
    match marker {
        Some(marker) => entry.line.time > marker,
        None => live,
    }
}

/// Counts a new live line. True when it is unread, the only lines that may
/// notify.
pub(super) fn count(buffer: &mut Buffer, key: &BufferKey, entry: &Entry) -> bool {
    tally(buffer, key, entry, true)
}

/// Counts an older line from history.
pub(super) fn count_history(buffer: &mut Buffer, key: &BufferKey, entry: &Entry) {
    tally(buffer, key, entry, false);
}

fn tally(buffer: &mut Buffer, key: &BufferKey, entry: &Entry, live: bool) -> bool {
    let counted = unread(key, entry, buffer.read_marker, live);
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
        if unread(&entry.line.buffer, entry, marker, true) {
            buffer.unread_entries.insert(entry.seq);
            buffer.highlights += usize::from(entry.highlight);
        }
    }
    buffer.unread = buffer.unread_entries.len();
}
