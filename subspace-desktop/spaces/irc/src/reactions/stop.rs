//! When a message's reaction choices carry the stop group.
//!
//! Stargate (`main` at `d4842b8`: `routeIrcStop` in `src/irc/routing.ts`
//! and its caller in `src/irc/ship.ts`) stops an agent's active turn when
//! a signed-in person reacts with one of its stop reactions to the request
//! or to an agent message answering it. The app offers the stop group only
//! where the conditions it can see hold:
//!
//! - the conversation is a channel (nothing here sends a private message);
//! - the nick the request asks is a fleet agent on the roster, so a person
//!   who posts an agent-style progress marker never gets `Stop <them>`;
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

use std::collections::{BTreeMap, HashMap};

use subspace_client_irc::{BufferKey, Target};

use crate::fleet::Roster;
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

/// What [`stops`] reads: the conversation, its held lines, their status
/// lines and the fleet roster.
pub(crate) struct Sources<'a> {
    pub buffer: &'a BufferKey,
    pub held: &'a Buffer,
    pub statuses: &'a Statuses,
    pub roster: &'a Roster,
}

/// Every agent message the buffer holds, for one sync. Each held message
/// ID is looked up once, so the work grows with the buffer, not its square.
pub(crate) fn stops(sources: &Sources) -> Stops {
    let Some(ids) = held_ids(sources) else {
        return Stops::new();
    };
    sources
        .held
        .entries
        .iter()
        .filter_map(|entry| Some((entry.seq, agent_message(sources, &ids, entry)?)))
        .collect()
}

/// What a stop reaction on the held message `msgid` concerns now, read
/// afresh from `sources` rather than from what a menu showed when it
/// opened: a stop is sent only while this still names a running agent.
pub(crate) fn concerning(sources: &Sources, msgid: &str) -> Option<AgentMessage> {
    let ids = held_ids(sources)?;
    let seq = *ids.get(msgid)?;
    let entry = sources.held.entries.iter().find(|entry| entry.seq == seq)?;
    agent_message(sources, &ids, entry)
}

/// Each held message ID's sequence, when the conversation can carry a
/// stop at all: a channel with status lines.
fn held_ids<'a>(sources: &Sources<'a>) -> Option<HashMap<&'a str, i64>> {
    if sources.statuses.is_empty() || !matches!(sources.buffer.target, Target::Channel(_)) {
        return None;
    }
    Some(
        sources
            .held
            .entries
            .iter()
            .filter_map(|entry| Some((entry.line.id.as_deref()?, entry.seq)))
            .collect(),
    )
}

/// What a stop reaction on `entry` would concern, when it is a request to
/// a roster agent with a status line, or that agent's verified reply.
fn agent_message(
    sources: &Sources,
    ids: &HashMap<&str, i64>,
    entry: &Entry,
) -> Option<AgentMessage> {
    let id = entry.line.id.as_deref()?;
    let (request, asked) = match sources.held.requests.asked(id) {
        Some(asked) => (entry.seq, asked),
        None => answered_request(sources.held, ids, entry)?,
    };
    sources.roster.agent(asked)?;
    sources.statuses.get(&request).map(|shown| AgentMessage {
        ship: shown.agent.clone(),
        running: shown.ticking(),
    })
}

/// The held request `entry` answers, by sequence, and the nick it asks,
/// when `entry` comes from that nick with its server account.
fn answered_request<'a>(
    held: &'a Buffer,
    ids: &HashMap<&str, i64>,
    entry: &Entry,
) -> Option<(i64, &'a str)> {
    let request = entry.line.reply_to.as_deref()?;
    let asked = held.requests.asked(request)?;
    let from = entry.line.from.as_deref()?;
    let account = entry.line.account.as_deref()?;
    let verified = from.eq_ignore_ascii_case(asked) && account.eq_ignore_ascii_case(asked);
    verified
        .then(|| Some((*ids.get(request)?, asked)))
        .flatten()
}

/// What is known once a stop reaction was sent: only that it was.
pub(crate) fn stop_sent(reaction: &str, ship: &str) -> String {
    format!(
        "Sent {reaction} to ask {ship} to stop. The status line shows the result when Stargate reports it."
    )
}

/// Why a stop reaction was not sent, and where the deliberate one is.
pub(crate) fn explanation(ship: Option<&str>) -> String {
    match ship {
        Some(ship) => format!(
            "To ask {ship} to stop, use Add reaction and choose under Stop {ship}, while its turn is running."
        ),
        None => "\u{270B} and \u{1F6D1} can stop an agent's turn, so Subspace sends them only from Add reaction, under Stop and the agent's name, on a running request to a fleet agent.".to_owned(),
    }
}
