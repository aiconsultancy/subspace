//! Each loaded request's newest trusted progress, and the status line that
//! shows. Rebuilt from the buffer's lines whenever it changes, so a restored
//! or reloaded thread reads the same as it did live. Endings are also kept
//! apart from the lines ([`crate::thread_status::Requests`]), with how and
//! when each request ended, so a finished request keeps its settled line
//! once the notice that ended it is no longer held.

use std::collections::BTreeMap;
use std::ops::Range;

use chrono::{DateTime, TimeDelta, Utc};
use subspace_client_irc::Line;

use super::requests::Ending;
use super::{Marker, Outcome, Stage, marker};
use crate::state::{Buffer, Entry};

/// The status line under each request that has trusted progress or has
/// ended, by the request line's sequence.
pub(crate) type Statuses = BTreeMap<i64, Shown>;

/// The most characters of the agent's nick a status line shows. The nick
/// comes from the network, so the line bounds it rather than trusting the
/// server's own limit.
const AGENT_CHARS: usize = 32;

/// The status line under a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    /// The agent's nick, as its newest progress notice spelt it, else as
    /// the request addressed it.
    pub agent: String,
    pub label: &'static str,
    /// The newest notice's time, when it may be out of date: the agent lost
    /// its event stream, the network is not connected, or the notice was
    /// already held when the network's current connection began.
    pub stale: Option<DateTime<Utc>>,
    /// The segment of the stage meter the request has reached.
    pub stage: Stage,
    /// When the request was sent, by the server: the line's clock counts
    /// from here.
    pub since: DateTime<Utc>,
    /// How the request ended, once it has: the line has settled.
    pub settled: Option<Settled>,
}

impl Shown {
    /// Whether the line is still running and current, so its clock ticks.
    pub(crate) fn ticking(&self) -> bool {
        self.settled.is_none() && self.stale.is_none()
    }
}

/// How a finished request's line reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Settled {
    /// How it ended; `None` for an ending stored before outcomes were
    /// kept.
    pub outcome: Option<Outcome>,
    /// From the request to the notice that ended it, when the ending's
    /// time is known.
    pub took: Option<TimeDelta>,
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
}

/// The status line of every request in `held` that is in progress or has
/// ended. `connected` holds the sequences already held when the network's
/// current connection began, `None` while it is down: notices among them,
/// or any notice while it is down, are stale. An ended request settles for
/// good: no other notice, older or later, reopens it.
pub(crate) fn statuses(held: &Buffer, connected: Option<&Range<i64>>) -> Statuses {
    let progress = progress(held);
    held.entries
        .iter()
        .filter_map(|entry| request_line(held, entry, &progress, connected))
        .collect()
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
pub(crate) fn agent_name(nick: &str) -> String {
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
/// replay never replaces a newer stage.
fn record<'a>(
    mut progress: Progress<'a>,
    (request, observation): (&'a str, Observation),
) -> Progress<'a> {
    let newer = progress
        .latest
        .get(request)
        .is_none_or(|held| observation.time >= held.time);
    if newer {
        progress.latest.insert(request, observation);
    }
    progress
}

/// The status line under `entry`, when it is a request that ended or has
/// trusted progress.
fn request_line(
    held: &Buffer,
    entry: &Entry,
    progress: &Progress<'_>,
    connected: Option<&Range<i64>>,
) -> Option<(i64, Shown)> {
    let id = entry.line.id.as_deref()?;
    let latest = progress.latest.get(id);
    let shown = match held.requests.ending(id) {
        Some(ending) => {
            let agent = latest
                .map(|observation| observation.agent.clone())
                .or_else(|| held.requests.asked(id).map(agent_name));
            settled_line(entry, agent.unwrap_or_default(), ending)
        }
        None => live_line(entry, latest?, connected)?,
    };
    Some((entry.seq, shown))
}

/// The line of a request in progress, from its newest observation.
fn live_line(
    entry: &Entry,
    observation: &Observation,
    connected: Option<&Range<i64>>,
) -> Option<Shown> {
    let phase = observation.marker.phase;
    Some(Shown {
        agent: observation.agent.clone(),
        label: phase.label(entry.line.own)?,
        stale: stale(observation, connected),
        stage: phase.stage(),
        since: entry.line.time,
        settled: None,
    })
}

/// The quiet line of a request that ended: done or failed, and how long it
/// took when the ending's time is known.
fn settled_line(entry: &Entry, agent: String, ending: &Ending) -> Shown {
    let took = ending
        .at
        .map(|at| at - entry.line.time)
        .filter(|took| *took >= TimeDelta::zero());
    Shown {
        agent,
        label: match ending.outcome {
            Some(Outcome::Failed) => "Failed",
            Some(Outcome::Completed) => "Done",
            None => "Ended",
        },
        stale: None,
        stage: Stage::Done,
        since: entry.line.time,
        settled: Some(Settled {
            outcome: ending.outcome,
            took,
        }),
    }
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
