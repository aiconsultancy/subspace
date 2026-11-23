//! One thread as the Thread tab shows it, built from a buffer's held lines
//! in one pass. Pure, so which messages belong and which steps count are
//! tested without GPUI.

use chrono::{DateTime, TimeDelta, Utc};
use subspace_client_irc::{BufferKey, ThreadRoots};

use crate::state::{Buffer, Entry, is_message};
use crate::thread_status::{self, Ending, Outcome, Phase, Stage};

/// A thread: its root, a request's timeline, and the replies in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Thread {
    pub root: Entry,
    /// The status timeline, when the root is a request with trusted
    /// progress or one that ended, as for its status line.
    pub timeline: Option<Timeline>,
    /// The latest replies the buffer holds, at most [`MAX_REPLIES`],
    /// oldest first.
    pub replies: Vec<Entry>,
    /// How many replies the buffer holds, all of them.
    pub reply_count: usize,
    /// The chosen message is itself a reply, such as a follow-up request:
    /// `replies` are its whole thread's.
    pub nested: bool,
}

/// The most replies a thread keeps to draw: the latest. A rebuild copies
/// no more, however long the thread.
pub(crate) const MAX_REPLIES: usize = 40;

/// A request's steps, from the progress notices its status line folds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Timeline {
    /// The agent the request asks, as its status line names it.
    pub agent: String,
    pub steps: Vec<Step>,
    /// The request ended, but no notice before its ending is held, such as
    /// after a restart that restored only part of the conversation.
    pub earlier_missing: bool,
}

/// One step the agent reported: a stage it reached, or how it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Step {
    pub kind: StepKind,
    /// When the step began, by the server; unknown for an ending stored
    /// before its time was kept.
    pub at: Option<DateTime<Utc>>,
    /// How long the step lasted, until the next began; for the ending, how
    /// long the whole request took. `None` while the step runs.
    pub took: Option<TimeDelta>,
}

/// What a step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepKind {
    /// A stage of the status line's meter before the end.
    Stage(Stage),
    /// The request ended; `None` for an ending stored before its outcome
    /// was kept.
    Ended(Option<Outcome>),
}

impl StepKind {
    /// The step's words.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Stage(Stage::Accepted) => "Accepted",
            Self::Stage(Stage::Working) => "Working",
            Self::Stage(Stage::Waiting) => "Waiting for an answer",
            Self::Stage(Stage::Reply) => "Writing the reply",
            Self::Stage(Stage::Done) | Self::Ended(Some(Outcome::Completed)) => "Done",
            Self::Ended(Some(Outcome::Failed)) => "Failed",
            Self::Ended(None) => "Ended",
        }
    }
}

/// The thread of message `root` in `held`, the buffer `buffer`; `None`
/// when the buffer does not hold it. Its replies are those of the
/// conversation's thread the message is in, as the transcript counts them,
/// so a follow-up request made as a reply shows the replies to it and a
/// reply sent from its tab appears there; its timeline is the chosen
/// message's own. The chosen message is shown once, as the root.
pub(crate) fn build(buffer: &BufferKey, held: &Buffer, root: &str) -> Option<Thread> {
    let root_entry = held
        .entries
        .iter()
        .find(|entry| entry.line.id.as_deref() == Some(root) && entry.line.buffer == *buffer)?;
    let roots = ThreadRoots::new(buffer, held.entries.iter().map(|entry| &entry.line));
    let thread = roots.root(&root_entry.line).unwrap_or(root);
    let all: Vec<&Entry> = held
        .entries
        .iter()
        .filter(|entry| entry.seq != root_entry.seq && is_reply(held, &roots, entry, thread))
        .collect();
    let reply_count = all.len();
    let replies = latest(&all);
    Some(Thread {
        root: root_entry.clone(),
        timeline: timeline(held, root_entry, root),
        replies,
        reply_count,
        nested: is_nested(thread, root),
    })
}

/// The latest [`MAX_REPLIES`] of `all`, copied.
fn latest(all: &[&Entry]) -> Vec<Entry> {
    all[all.len().saturating_sub(MAX_REPLIES)..]
        .iter()
        .map(|entry| (*entry).clone())
        .collect()
}

/// Whether the chosen message `root` is itself a reply in `thread`.
fn is_nested(thread: &str, root: &str) -> bool {
    thread != root
}

/// Whether `entry` is a reply in the thread of `root`: a message, not
/// folded progress, whose reply tag leads to the root. As the transcript's
/// reply count reads it.
fn is_reply<'a>(held: &Buffer, roots: &ThreadRoots<'a>, entry: &'a Entry, root: &str) -> bool {
    let line = &entry.line;
    line.reply_to.is_some()
        && is_message(line.kind)
        && !held.folded.contains(&entry.seq)
        && roots.root(line) == Some(root)
}

/// The request's timeline, when `root` is a request.
fn timeline(held: &Buffer, root_entry: &Entry, root: &str) -> Option<Timeline> {
    let asked = held.requests.asked(root);
    let ending = held.requests.ending(root);
    if asked.is_none() && ending.is_none() {
        return None;
    }
    let mut observed = observed(held, root);
    if let Some(ending) = ending {
        // The kept ending is the request's end, as its status line says:
        // no held notice ends it otherwise, and none after it is a step.
        observed.retain(|notice| {
            !matches!(notice.phase, Phase::Finished(_))
                && ending.at.is_none_or(|at| notice.at <= at)
        });
    }
    if observed.is_empty() && ending.is_none() {
        // Addressed, as `Morning, all.` reads, but no agent reported on it:
        // the transcript shows no status line either.
        return None;
    }
    let agent = observed
        .last()
        .map(|notice| notice.from.clone())
        .or_else(|| asked.map(thread_status::agent_name))
        .unwrap_or_default();
    let mut steps = steps(&observed, root_entry.line.time);
    // Only the ending held, as after a partial history load: the steps
    // before it are not loaded.
    let earlier_missing = ending.is_some() && observed.is_empty();
    if let Some(ending) = ending {
        settle(&mut steps, ending, root_entry.line.time);
    }
    Some(Timeline {
        agent,
        steps,
        earlier_missing,
    })
}

/// One folded notice on the request.
struct Observed {
    phase: Phase,
    at: DateTime<Utc>,
    seq: i64,
    from: String,
}

/// The folded notices on request `root`, in server time, then buffer order.
/// Only folded notices count: the buffer folds a notice only when its
/// server account is the nick the held request asks.
fn observed(held: &Buffer, root: &str) -> Vec<Observed> {
    let mut observed: Vec<Observed> = held
        .entries
        .iter()
        .filter(|entry| held.folded.contains(&entry.seq))
        .filter(|entry| entry.line.reply_to.as_deref() == Some(root))
        .filter_map(|entry| {
            let marker = thread_status::marker(&entry.line)?;
            Some(Observed {
                phase: marker.phase,
                at: entry.line.time,
                seq: entry.seq,
                from: thread_status::agent_name(entry.line.from.as_deref().unwrap_or_default()),
            })
        })
        .collect();
    observed.sort_by_key(|notice| (notice.at, notice.seq));
    observed
}

/// The steps the notices report: one per change of stage on the meter,
/// ending at the first notice that ended the request. Each step lasts until
/// the next; the ending reports the request's whole time from `since`.
fn steps(observed: &[Observed], since: DateTime<Utc>) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    for notice in observed {
        let kind = match notice.phase {
            Phase::Finished(outcome) => StepKind::Ended(Some(outcome)),
            phase => StepKind::Stage(phase.stage()),
        };
        if steps.last().is_some_and(|last| last.kind == kind) {
            continue;
        }
        if let Some(last) = steps.last_mut() {
            last.took = last.at.map(|at| notice.at - at);
        }
        let ended = matches!(kind, StepKind::Ended(_));
        steps.push(Step {
            kind,
            at: Some(notice.at),
            took: ended.then(|| notice.at - since),
        });
        if ended {
            break;
        }
    }
    steps
}

/// Ends `steps` with the request's kept `ending` when no held notice did,
/// as after the ending notice is no longer held.
fn settle(steps: &mut Vec<Step>, ending: &Ending, since: DateTime<Utc>) {
    if steps
        .last()
        .is_some_and(|last| matches!(last.kind, StepKind::Ended(_)))
    {
        return;
    }
    if let (Some(last), Some(at)) = (steps.last_mut(), ending.at) {
        last.took = last.at.map(|began| at - began);
    }
    steps.push(Step {
        kind: StepKind::Ended(ending.outcome),
        at: ending.at,
        took: ending.at.map(|at| at - since),
    });
}
