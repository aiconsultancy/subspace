//! One live status line per agent request, from the progress notices an
//! agent gateway (Stargate) sends as IRC replies to the request. Pure, so
//! the rules are tested without GPUI.
//!
//! A progress notice is a `NOTICE` carrying the client-only tag
//! [`PROGRESS_TAG`] and `+draft/reply=<the request's msgid>`. Anyone can send
//! a client-only tag, so a tagged notice folds into its request's status
//! line only when the server vouches for its sender ([`trust`]). Everything
//! else, including questions, answer instructions, final replies and
//! untrusted or unrecognised notices, stays an ordinary message, and no text
//! is ever read for state. See `docs/changes/irc-thread-status/spec.md`.

mod model;
mod requests;
#[cfg(test)]
mod tests;
mod trust;

use subspace_client_irc::{Line, LineKind, Target};

pub(crate) use model::{Shown, Statuses, shown_entry, statuses};
pub(crate) use requests::{Arrival, Kept, REMEMBERED, Requests};
pub(crate) use trust::{arrival, asked, notices};

/// The client-only tag that marks request progress on a `NOTICE`. Its value
/// is a stage; a stage before the end may carry `,disconnected`. The IRC
/// client keeps it, among the few client-only tags it keeps at all.
pub(crate) const PROGRESS_TAG: &str = subspace_client_irc::PROGRESS_TAG;

/// The value suffix by which the agent says it lost its own event stream.
const DISCONNECTED: &str = "disconnected";

/// Where a request stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// `received`, `submitting`: the agent's runtime has not confirmed
    /// receipt.
    Unconfirmed,
    /// `accepted`, `delivered`: accepted, with no work seen yet.
    Accepted,
    /// `queued`: waiting behind other work.
    Queued,
    /// `model`, `reasoning`.
    Thinking,
    /// `tool`.
    Tool,
    /// `retry`: waiting to try again.
    Retrying,
    /// `waiting`: the agent asked a question and waits for a person.
    Waiting,
    /// `clarified`: the answer arrived.
    Answered,
    /// `reply`.
    Writing,
    /// `completed`, `failed`: the request ended.
    Finished,
}

/// What one progress notice says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Marker {
    pub phase: Phase,
    /// The agent lost its own event stream, so the stage may be out of date.
    pub disconnected: bool,
}

impl Phase {
    /// What the status line says; `None` once the request ended. `own` says
    /// the request is ours, so a question waits for our answer.
    pub(crate) fn label(self, own: bool) -> Option<&'static str> {
        Some(match self {
            Self::Unconfirmed => "Receipt unconfirmed",
            Self::Accepted => "Accepted",
            Self::Queued => "Queued",
            Self::Thinking => "Thinking",
            Self::Tool => "Using a tool",
            Self::Retrying => "Waiting to retry",
            Self::Waiting if own => "Waiting for your answer",
            Self::Waiting => "Waiting for an answer",
            Self::Answered => "Answer received",
            Self::Writing => "Writing a reply",
            Self::Finished => return None,
        })
    }
}

/// The progress marker on `line`: a `NOTICE` from someone else, in a
/// channel or private conversation, replying to a message, whose tag value
/// this board knows. It says nothing about whether the sender is trusted.
pub(crate) fn marker(line: &Line) -> Option<Marker> {
    tagged(line).and_then(parse)
}

/// The progress tag's value on a line that may carry a marker.
fn tagged(line: &Line) -> Option<&str> {
    let eligible = line.kind == LineKind::Notice
        && !line.own
        && line.reply_to.as_deref().is_some_and(|id| !id.is_empty())
        && line.buffer.target != Target::Server;
    eligible
        .then(|| line.client_tags.get(PROGRESS_TAG))
        .flatten()
        .map(String::as_str)
}

/// `stage`, or `stage,disconnected` before the end. Anything else,
/// including an empty suffix such as `completed,`, is not a marker, so the
/// notice stays an ordinary message.
fn parse(value: &str) -> Option<Marker> {
    let (stage, disconnected) = match value.split_once(',') {
        None => (value, false),
        Some((stage, DISCONNECTED)) => (stage, true),
        Some(_) => return None,
    };
    let phase = match stage {
        "received" | "submitting" => Phase::Unconfirmed,
        "accepted" | "delivered" => Phase::Accepted,
        "queued" => Phase::Queued,
        "model" | "reasoning" => Phase::Thinking,
        "tool" => Phase::Tool,
        "retry" => Phase::Retrying,
        "waiting" => Phase::Waiting,
        "clarified" => Phase::Answered,
        "reply" => Phase::Writing,
        "completed" | "failed" => Phase::Finished,
        _ => return None,
    };
    let ended_while_lost = disconnected && phase == Phase::Finished;
    (!ended_while_lost).then_some(Marker {
        phase,
        disconnected,
    })
}
