//! When a message's reaction choices carry the stop group.
//!
//! Stargate (`main` at `d4842b8`: `routeIrcStop` in `src/irc/routing.ts`
//! and its caller in `src/irc/ship.ts`) stops an agent's active turn when
//! a signed-in person reacts with one of its stop reactions to the request
//! or to an agent message answering it. The app offers the stop group only
//! where the conditions it can see hold:
//!
//! - the conversation is a channel (nothing here sends a private message);
//! - the message is a held request whose status line is trusted (its
//!   progress comes from the account named after the nick it asks, so the
//!   agent is verified) and still running and current, not settled and not
//!   stale; or
//! - the message is that agent's own reply to such a request: its nick and
//!   its server `account` both name the nick the request asks. An
//!   unverified nick is never taken for the agent.
//!
//! What it cannot see, such as the gateway's `stopReactions` setting, its
//! access rules or whether our account counts as a person, is left to the
//! status line, which shows what Stargate reports.

use std::collections::BTreeMap;

use subspace_client_irc::{BufferKey, Target};

use crate::state::{Buffer, Entry};
use crate::thread_status::Statuses;

/// A message a stop reaction concerns: a request to a verified agent, or
/// that agent's reply to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentMessage {
    /// The agent, as the request's status line names it.
    pub ship: String,
    /// The status line says the turn is still running and current, so the
    /// stop group belongs in the message's choices.
    pub running: bool,
}

/// The agent messages among a buffer's lines, by sequence.
pub(crate) type Stops = BTreeMap<i64, AgentMessage>;

/// Every agent message `held` holds, for one sync.
pub(crate) fn stops(buffer: &BufferKey, held: &Buffer, statuses: &Statuses) -> Stops {
    if statuses.is_empty() {
        return Stops::new();
    }
    held.entries
        .iter()
        .filter_map(|entry| Some((entry.seq, agent_message(buffer, held, statuses, entry)?)))
        .collect()
}

/// What a stop reaction on `entry` would concern, when it is an agent
/// message in a channel.
pub(crate) fn agent_message(
    buffer: &BufferKey,
    held: &Buffer,
    statuses: &Statuses,
    entry: &Entry,
) -> Option<AgentMessage> {
    let channel = matches!(buffer.target, Target::Channel(_));
    let id = entry.line.id.as_deref().filter(|_| channel)?;
    let request = match held.requests.asked(id) {
        Some(_) => entry,
        None => answered_request(held, entry)?,
    };
    statuses.get(&request.seq).map(|shown| AgentMessage {
        ship: shown.agent.clone(),
        running: shown.ticking(),
    })
}

/// The held request `entry` answers, when `entry` comes from the verified
/// agent that request asks.
fn answered_request<'a>(held: &'a Buffer, entry: &Entry) -> Option<&'a Entry> {
    let request = entry.line.reply_to.as_deref()?;
    let asked = held.requests.asked(request)?;
    let from = entry.line.from.as_deref()?;
    let account = entry.line.account.as_deref()?;
    let verified = from.eq_ignore_ascii_case(asked) && account.eq_ignore_ascii_case(asked);
    verified
        .then(|| {
            held.entries
                .iter()
                .find(|held| held.line.id.as_deref() == Some(request))
        })
        .flatten()
}
