//! Lines that belong among the held ones and the dedupe every added line
//! goes through: by `msgid`, then by time, sender and text.

use std::collections::{HashSet, VecDeque};

use subspace_client_irc::{BufferKey, Line};

use super::{Entry, HISTORY_MAX, IrcState, LINE_CAP, counts};

/// The buffer already holds this line, such as a JOIN that history repeats
/// right after joining, or a message restored from the store that a bouncer
/// replays ([`Seen`]).
pub(super) fn known(state: &IrcState, line: &Line) -> bool {
    state
        .buffers
        .get(&line.buffer)
        .is_some_and(|buffer| Seen::of(&buffer.entries).knows(line))
}

/// A line's identity without a `msgid`: its time, sender and text.
type Fallback = (i64, Option<String>, String);

fn fallback(line: &Line) -> Fallback {
    (
        line.time.timestamp_millis(),
        line.from.as_ref().map(|from| from.to_ascii_lowercase()),
        line.text.clone(),
    )
}

/// The lines a buffer holds, for dedupe: by `msgid`, then, where either
/// copy has none, by time, sender and text.
#[derive(Default)]
struct Seen {
    ids: HashSet<String>,
    /// Every line by time, sender and text.
    all: HashSet<Fallback>,
    /// Lines without a `msgid`, by time, sender and text.
    anonymous: HashSet<Fallback>,
}

impl Seen {
    fn of(entries: &VecDeque<Entry>) -> Self {
        let mut seen = Self::default();
        entries.iter().for_each(|entry| seen.add(&entry.line));
        seen
    }

    fn knows(&self, line: &Line) -> bool {
        let key = fallback(line);
        match &line.id {
            Some(id) => self.ids.contains(id) || self.anonymous.contains(&key),
            None => self.all.contains(&key),
        }
    }

    fn add(&mut self, line: &Line) {
        let key = fallback(line);
        match &line.id {
            Some(id) => _ = self.ids.insert(id.clone()),
            None => _ = self.anonymous.insert(key.clone()),
        }
        self.all.insert(key);
    }
}

/// Lines that belong among the held ones, not before them: what was missed
/// since the newest stored line, the lines around one message, or lines
/// restored from the store. Each goes after the held lines of its time.
/// Lines held already and repeats within `lines` are skipped ([`Seen`]).
/// Only `count`ed lines add to the unread count, as history does; none
/// notifies. A merge never passes `HISTORY_MAX`: the oldest fresh lines go
/// first, and nothing held is dropped.
pub(super) fn merge(state: &mut IrcState, key: &BufferKey, lines: Vec<Line>, count: bool) {
    let words = state.highlights.clone();
    let held = state.buffers.entry(key.clone()).or_default();
    let fresh = fresh(&held.entries, lines);
    let room = HISTORY_MAX.saturating_sub(held.entries.len());
    let skip = fresh.len().saturating_sub(room);
    let fresh: Vec<Line> = fresh.into_iter().skip(skip).collect();
    let first = state.next_seq;
    state.next_seq += fresh.len() as i64;
    let buffer = state.buffers.entry(key.clone()).or_default();
    for (seq, line) in (first..).zip(fresh) {
        let highlight = super::is_highlight(&line, &words);
        let entry = Entry {
            seq,
            line,
            highlight,
        };
        if count {
            counts::count_history(buffer, key, &entry);
        }
        let at = buffer
            .entries
            .partition_point(|held| held.line.time <= entry.line.time);
        buffer.entries.insert(at, entry);
    }
    buffer.history.extra = buffer.entries.len().saturating_sub(LINE_CAP);
}

/// The lines of `lines` that `entries` do not hold, without repeats.
fn fresh(entries: &VecDeque<Entry>, lines: Vec<Line>) -> Vec<Line> {
    let mut seen = Seen::of(entries);
    lines
        .into_iter()
        .filter(|line| {
            let new = !seen.knows(line);
            seen.add(line);
            new
        })
        .collect()
}
