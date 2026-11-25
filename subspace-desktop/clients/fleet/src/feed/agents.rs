//! The officers whose turns a feed's stream subscribes to with `?agent=`,
//! and the `agent-activity` events it kept for each, in memory only.
//!
//! The feed never opens a second stream for them: Stargate serves at most
//! eight streams in all, so the fleet's one stream is opened again with the
//! officers in its query whenever the set changes, and without them once
//! nothing watches an officer. Past eight, an officer is left out of the
//! query and its reader is told it is refused, so its view reads the
//! snapshot on a timer instead.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::Error;
use crate::activity::{ActivityNews, AgentActivityEvent, MAX_HELD_BYTES, MAX_HELD_EVENTS};

/// Stargate takes at most eight `agent` parameters on one stream.
pub(crate) const MAX_AGENTS: usize = 8;
/// The most events kept for one officer that its reader has not taken: a
/// reader that falls further behind is told it missed some and fetches
/// the snapshot again.
const MAX_EVENTS: usize = MAX_HELD_EVENTS;
/// The most bytes of event JSON kept for one officer, as for
/// [`MAX_EVENTS`].
const MAX_EVENT_BYTES: usize = MAX_HELD_BYTES;

/// What a feed knows of its watched officers.
#[derive(Default)]
pub(crate) struct Agents {
    /// The officers watched; the stream subscribes to the first
    /// [`MAX_AGENTS`] it has not refused.
    wanted: BTreeSet<String>,
    /// Officers the stream refused to subscribe to, left out of its query.
    refused: BTreeSet<String>,
    logs: BTreeMap<String, Log>,
    /// Streams opened so far, so each connection has its own number.
    connections: u64,
}

/// One officer's kept events.
#[derive(Default)]
struct Log {
    /// The open stream that carries this officer's events.
    connection: Option<u64>,
    /// The position of `events`' first.
    first: u64,
    events: VecDeque<AgentActivityEvent>,
    /// The bytes of JSON `events` were read from.
    bytes: usize,
}

impl Agents {
    /// Watches exactly `agents`; whether the stream's query changes.
    pub(crate) fn set(&mut self, wanted: BTreeSet<String>) -> bool {
        if wanted == self.wanted {
            return false;
        }
        let before = self.subscribed();
        self.logs.retain(|agent, _| wanted.contains(agent));
        self.refused.retain(|agent| wanted.contains(agent));
        for agent in &wanted {
            self.logs.entry(agent.clone()).or_default();
        }
        self.wanted = wanted;
        before != self.subscribed()
    }

    /// The officers the next stream subscribes to: the first
    /// [`MAX_AGENTS`] in order that it has not refused.
    pub(crate) fn subscribed(&self) -> Vec<String> {
        self.wanted
            .difference(&self.refused)
            .take(MAX_AGENTS)
            .cloned()
            .collect()
    }

    /// Whether no stream will carry `agent`: Stargate refused it, or it is
    /// past the [`MAX_AGENTS`] a stream takes.
    fn unsubscribed(&self, agent: &str) -> bool {
        self.refused.contains(agent)
            || !self
                .wanted
                .difference(&self.refused)
                .take(MAX_AGENTS)
                .any(|subscribed| subscribed == agent)
    }

    /// A stream opened subscribed to `subscribed`.
    pub(crate) fn connected(&mut self, subscribed: &[String]) {
        self.connections = self.connections.wrapping_add(1);
        let connection = self.connections;
        for (agent, log) in &mut self.logs {
            log.connection = subscribed.contains(agent).then_some(connection);
        }
    }

    /// The stream closed: no events come until the next.
    pub(crate) fn disconnected(&mut self) {
        for log in self.logs.values_mut() {
            log.connection = None;
        }
    }

    /// When Stargate refused with 400 or 404 (an officer it does not know)
    /// a stream that asked for the officers `attempted`, leaves those still
    /// watched out of the next one; whether the stream asked for any, so it
    /// opens again at once. The refusal is about the officers that stream
    /// asked for, never those watched since: they are asked for next.
    pub(crate) fn refuse(&mut self, error: Error, attempted: &[String]) -> bool {
        if !matches!(error, Error::NotFound | Error::UnexpectedStatus(400)) {
            return false;
        }
        let still = attempted
            .iter()
            .filter(|agent| self.wanted.contains(*agent));
        self.refused.extend(still.cloned());
        !attempted.is_empty()
    }

    /// Keeps `event` for its officer while watched; whether it was kept.
    pub(crate) fn push(&mut self, event: AgentActivityEvent) -> bool {
        let Some(log) = self.logs.get_mut(&event.agent_id) else {
            return false;
        };
        log.bytes = log.bytes.saturating_add(event.bytes);
        log.events.push_back(event);
        while log.events.len() > MAX_EVENTS || log.bytes > MAX_EVENT_BYTES {
            let Some(oldest) = log.events.pop_front() else {
                break;
            };
            log.bytes = log.bytes.saturating_sub(oldest.bytes);
            log.first = log.first.wrapping_add(1);
        }
        true
    }

    /// What a reader at position `after` has not taken of `agent`'s events.
    pub(crate) fn news(&self, agent: &str, after: u64) -> Option<ActivityNews> {
        let log = self.logs.get(agent)?;
        let end = log.first + log.events.len() as u64;
        let start = after.clamp(log.first, end);
        let skip = usize::try_from(start - log.first).unwrap_or(usize::MAX);
        Some(ActivityNews {
            connection: log.connection,
            events: log.events.iter().skip(skip).cloned().collect(),
            next: end,
            missed: after < log.first,
            refused: self.unsubscribed(agent),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Agents, MAX_AGENTS, MAX_EVENT_BYTES, MAX_EVENTS};
    use crate::activity::{ActivityJoin, AgentActivityEvent};
    use crate::{Error, FleetState, Link};

    fn set(agents: &[&str]) -> BTreeSet<String> {
        agents.iter().map(|agent| (*agent).to_owned()).collect()
    }

    fn event(agent: &str, seq: u64) -> AgentActivityEvent {
        serde_json::from_value(serde_json::json!({
            "agentId": agent, "turnId": "t1", "generation": "g1", "seq": seq, "at": 1,
            "change": {"type": "turn-updated", "turn": {
                "id": "t1", "seq": seq, "status": "running", "startedAt": 1, "updatedAt": 1,
                "endedAt": null, "reconnects": 0, "usage": null}}
        }))
        .unwrap()
    }

    #[test]
    fn the_query_changes_only_with_the_subscribed_officers() {
        let mut agents = Agents::default();
        assert!(agents.set(set(&["example-lyra"])));
        assert!(!agents.set(set(&["example-lyra"])));
        let asked = agents.subscribed();
        assert!(
            !agents.refuse(Error::UnexpectedStatus(503), &asked),
            "only 400 or 404"
        );
        assert!(agents.refuse(Error::NotFound, &asked));
        assert!(agents.subscribed().is_empty());
        assert!(
            !agents.refuse(Error::NotFound, &agents.subscribed()),
            "nothing left to refuse"
        );
        assert!(agents.news("example-lyra", 0).unwrap().refused);
        let many: Vec<String> = (0..10).map(|n| format!("example-{n}")).collect();
        assert!(agents.set(many.iter().cloned().collect()));
        assert_eq!(agents.subscribed().len(), MAX_AGENTS);
        assert!(agents.news("example-9", 0).is_some_and(|news| news.refused));
        assert!(agents.set(BTreeSet::new()));
        assert!(agents.news("example-0", 0).is_none());
    }

    /// A stream that asked for A is refused only after A was replaced by
    /// B: the refusal is about A, and B is asked for next.
    #[test]
    fn a_late_refusal_leaves_out_only_the_officers_its_stream_asked_for() {
        let mut agents = Agents::default();
        agents.set(set(&["example-a"]));
        let asked = agents.subscribed();
        assert!(agents.set(set(&["example-b"])));
        assert!(
            agents.refuse(Error::NotFound, &asked),
            "opens again at once"
        );
        assert_eq!(agents.subscribed(), ["example-b"]);
        assert!(!agents.news("example-b", 0).unwrap().refused);
        // Watched again, A is asked for again: its refusal went with it.
        agents.set(set(&["example-a", "example-b"]));
        assert_eq!(agents.subscribed(), ["example-a", "example-b"]);
    }

    #[test]
    fn a_reader_takes_events_once_and_learns_of_connections_and_losses() {
        let mut agents = Agents::default();
        agents.set(set(&["example-lyra"]));
        assert_eq!(agents.news("example-lyra", 0).unwrap().connection, None);
        agents.connected(&agents.subscribed());
        assert!(!agents.push(event("example-other", 1)));
        assert!(agents.push(event("example-lyra", 1)));
        let news = agents.news("example-lyra", 0).unwrap();
        assert_eq!(
            (news.connection, news.events.len(), news.next),
            (Some(1), 1, 1)
        );
        assert!(agents.news("example-lyra", 1).unwrap().events.is_empty());
        agents.disconnected();
        assert_eq!(agents.news("example-lyra", 1).unwrap().connection, None);
        for seq in 2..(MAX_EVENTS as u64 + 10) {
            agents.push(event("example-lyra", seq));
        }
        let news = agents.news("example-lyra", 1).unwrap();
        assert!(news.missed);
        assert_eq!(news.events.len(), MAX_EVENTS);
        assert!(!agents.news("example-lyra", news.next).unwrap().missed);
    }

    #[test]
    fn a_ninth_officer_is_left_out_of_the_query_and_read_without_a_stream() {
        let mut agents = Agents::default();
        let nine: Vec<String> = (1..=9).map(|n| format!("example-{n}")).collect();
        assert!(agents.set(nine.iter().cloned().collect()));
        let subscribed = agents.subscribed();
        assert_eq!(subscribed.len(), MAX_AGENTS);
        assert!(!subscribed.contains(&nine[8]), "{subscribed:?}");
        agents.connected(&subscribed);
        for agent in &nine[..8] {
            let news = agents.news(agent, 0).unwrap();
            assert!(!news.refused && news.connection.is_some());
        }
        let ninth = agents.news(&nine[8], 0).unwrap();
        assert!(ninth.refused && ninth.connection.is_none());
        // Its view fetches without waiting for a stream that will not
        // come, though the fleet's stream is live.
        let mut join = ActivityJoin::new();
        join.set_fleet_state(&FleetState::Followed(Link::Live));
        assert!(!join.start_fetch());
        join.absorb(ninth);
        assert!(join.start_fetch());
        // Once one of the eight goes, the ninth is subscribed.
        assert!(agents.set(nine[1..].iter().cloned().collect()));
        assert!(agents.subscribed().contains(&nine[8]));
        assert!(!agents.news(&nine[8], 0).unwrap().refused);
    }

    #[test]
    fn an_officers_kept_events_are_bounded_by_bytes_too() {
        let mut agents = Agents::default();
        agents.set(set(&["example-lyra"]));
        for seq in 0..MAX_EVENTS as u64 {
            let mut big = event("example-lyra", seq);
            big.bytes = 60 * 1024;
            assert!(agents.push(big));
        }
        let news = agents.news("example-lyra", 0).unwrap();
        let kept: usize = news.events.iter().map(|event| event.bytes).sum();
        assert!(kept <= MAX_EVENT_BYTES, "{kept} bytes kept");
        assert_eq!(news.events.len(), MAX_EVENT_BYTES / (60 * 1024));
        assert!(news.missed, "the reader learns it fell behind");
        assert_eq!(
            news.events.last().map(|event| event.seq),
            Some(MAX_EVENTS as u64 - 1)
        );
    }
}
