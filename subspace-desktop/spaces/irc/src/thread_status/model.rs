//! Each loaded request's newest trusted progress, and the status line that
//! shows. Rebuilt from the buffer's lines whenever it changes, so a restored
//! or reloaded thread reads the same as it did live. Endings are also kept
//! apart from the lines ([`crate::thread_status::Requests`]), so a request
//! stays ended once the notice that ended it is no longer held.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use chrono::{DateTime, Utc};
use subspace_client_irc::Line;

use super::{Marker, Phase, marker};
use crate::state::{Buffer, Entry};

/// The status line under each request in progress, by the request line's
/// sequence.
pub(crate) type Statuses = BTreeMap<i64, Shown>;

/// The most characters of the agent's nick a status line shows. The nick
/// comes from the network, so the line bounds it rather than trusting the
/// server's own limit.
const AGENT_CHARS: usize = 32;

/// The status line under a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    /// The agent's nick, as its newest progress notice spelt it.
    pub agent: String,
    pub label: &'static str,
    /// The newest notice's time, when it may be out of date: the agent lost
    /// its event stream, the network is not connected, or the notice was
    /// already held when the network's current connection began.
    pub stale: Option<DateTime<Utc>>,
}

/// One folded progress notice.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Observation {
    agent: String,
    marker: Marker,
    time: DateTime<Utc>,
    seq: i64,
}

/// What the folded progress says, by request message ID.
#[derive(Default)]
struct Progress<'a> {
    /// The newest observation of each request by server time; of two at the
    /// same time, the later in the buffer.
    latest: BTreeMap<&'a str, Observation>,
    /// Requests that completed or failed. No other notice, older or later,
    /// reopens them.
    ended: BTreeSet<&'a str>,
}

/// The status line of every request in `held` that is still in progress.
/// `connected` holds the sequences already held when the network's current
/// connection began, `None` while it is down: notices among them, or any
/// notice while it is down, are stale.
pub(crate) fn statuses(held: &Buffer, connected: Option<&Range<i64>>) -> Statuses {
    let progress = progress(held);
    held.entries
        .iter()
        .filter(|entry| !ended(held, entry))
        .filter_map(|entry| request_line(entry, &progress, connected))
        .collect()
}

/// Whether `entry` is a request that ended, also when the notice that
/// ended it is no longer held.
fn ended(held: &Buffer, entry: &Entry) -> bool {
    entry
        .line
        .id
        .as_deref()
        .is_some_and(|id| held.requests.ended(id))
}

/// The folded notices of `held`, each against the request it answers.
fn progress(held: &Buffer) -> Progress<'_> {
    held.entries
        .iter()
        .filter(|entry| held.folded.contains(&entry.seq))
        .filter_map(observation)
        .fold(Progress::default(), record)
}

/// `entry`'s marker, with the message ID of the request it answers.
fn observation(entry: &Entry) -> Option<(&str, Observation)> {
    let marker = marker(&entry.line)?;
    let request = entry.line.reply_to.as_deref()?;
    let agent = agent_name(entry.line.from.as_deref()?);
    Some((
        request,
        Observation {
            agent,
            marker,
            time: entry.line.time,
            seq: entry.seq,
        },
    ))
}

/// `nick` as one short line of plain text: cleaned as status text is
/// ([`subspace_client_irc::status_text`]: IRC formatting, controls and bidi
/// overrides removed), without whitespace or invisible format characters,
/// and cut to [`AGENT_CHARS`] with an ellipsis.
pub(super) fn agent_name(nick: &str) -> String {
    let clean = subspace_client_irc::status_text(nick).unwrap_or_default();
    let mut plain = clean
        .chars()
        .filter(|c| !c.is_whitespace() && !invisible(*c));
    let mut name: String = plain.by_ref().take(AGENT_CHARS).collect();
    if plain.next().is_some() {
        name.pop();
        name.push('…');
    }
    name
}

/// Zero-width, joining, direction and other format characters, which would
/// let a nick hide text or read as another.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{61c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    )
}

/// Keeps each request's newest observation by server time, so an older
/// replay never replaces a newer stage, and an ending for good.
fn record<'a>(
    mut progress: Progress<'a>,
    (request, observation): (&'a str, Observation),
) -> Progress<'a> {
    if observation.marker.phase == Phase::Finished {
        progress.ended.insert(request);
    }
    let newer = progress
        .latest
        .get(request)
        .is_none_or(|held| observation.time >= held.time);
    if newer {
        progress.latest.insert(request, observation);
    }
    progress
}

/// The status line under `entry`, when it is a request still in progress.
fn request_line(
    entry: &Entry,
    progress: &Progress<'_>,
    connected: Option<&Range<i64>>,
) -> Option<(i64, Shown)> {
    let id = entry
        .line
        .id
        .as_deref()
        .filter(|id| !progress.ended.contains(*id))?;
    let observation = progress.latest.get(id)?;
    let label = observation.marker.phase.label(entry.line.own)?;
    let shown = Shown {
        agent: observation.agent.clone(),
        label,
        stale: stale(observation, connected),
    };
    Some((entry.seq, shown))
}

/// The notice's time when it may be out of date: the agent reported losing
/// its stream, the network is down, or the notice was already held when the
/// current connection began.
fn stale(observation: &Observation, connected: Option<&Range<i64>>) -> Option<DateTime<Utc>> {
    let earlier = connected.is_none_or(|held| held.contains(&observation.seq));
    (observation.marker.disconnected || earlier).then_some(observation.time)
}

/// Where the line numbered `seq` shows in `held`, as a position and a
/// sequence: the line itself, or, for folded progress, the held request it
/// answers. `None` when neither is held.
pub(crate) fn shown_entry(held: &Buffer, seq: i64) -> Option<(usize, i64)> {
    let at = held.entries.iter().position(|entry| entry.seq == seq)?;
    let shown = shown_position(held, at)?;
    Some((shown, held.entries.get(shown)?.seq))
}

fn shown_position(held: &Buffer, at: usize) -> Option<usize> {
    let entry = held.entries.get(at)?;
    (!held.folded.contains(&entry.seq))
        .then_some(at)
        .or_else(|| request_position(held, &entry.line))
}

/// The held request that `line` replies to.
fn request_position(held: &Buffer, line: &Line) -> Option<usize> {
    let request = line.reply_to.as_deref()?;
    held.entries
        .iter()
        .position(|entry| entry.line.id.as_deref() == Some(request))
}
