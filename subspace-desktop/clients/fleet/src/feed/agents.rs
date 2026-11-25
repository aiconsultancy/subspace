//! The officers whose turns a feed's stream subscribes to with `?agent=`,
//! and the `agent-activity` events it kept for each, in memory only.
//!
//! The feed never opens a second stream for them: Stargate serves at most
//! eight streams in all, so the fleet's one stream is opened again with the
//! officers in its query whenever the set changes, and without them once
//! nothing watches an officer.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::Error;
use crate::activity::{ActivityNews, AgentActivityEvent};

/// Stargate takes at most eight `agent` parameters on one stream.
pub(crate) const MAX_AGENTS: usize = 8;
/// The most events kept for one officer that its reader has not taken: a
/// reader that falls further behind is told it missed some and fetches
/// the snapshot again.
const MAX_EVENTS: usize = 512;

/// What a feed knows of its watched officers.
#[derive(Default)]
pub(crate) struct Agents {
    /// The officers to subscribe to, at most [`MAX_AGENTS`].
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
}

impl Agents {
    /// Watches exactly `agents`, the first [`MAX_AGENTS`] in order; whether
    /// the stream's query changes.
    pub(crate) fn set(&mut self, agents: BTreeSet<String>) -> bool {
        let wanted: BTreeSet<String> = agents.into_iter().take(MAX_AGENTS).collect();
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

    /// The officers the next stream subscribes to.
    pub(crate) fn subscribed(&self) -> Vec<String> {
        self.wanted.difference(&self.refused).cloned().collect()
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

    /// When Stargate refused a stream that subscribed to officers with 400
    /// or 404 (an officer it does not know), leaves them out of the next
    /// one; whether it did, so the stream opens again at once.
    pub(crate) fn refuse(&mut self, error: Error) -> bool {
        if !matches!(error, Error::NotFound | Error::UnexpectedStatus(400)) {
            return false;
        }
        let subscribed = self.subscribed();
        self.refused.extend(subscribed.iter().cloned());
        !subscribed.is_empty()
    }

    /// Keeps `event` for its officer while watched; whether it was kept.
    pub(crate) fn push(&mut self, event: AgentActivityEvent) -> bool {
        let Some(log) = self.logs.get_mut(&event.agent_id) else {
            return false;
        };
        log.events.push_back(event);
        while log.events.len() > MAX_EVENTS {
            log.events.pop_front();
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
            refused: self.refused.contains(agent),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Agents, MAX_AGENTS, MAX_EVENTS};
    use crate::Error;
    use crate::activity::AgentActivityEvent;

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
        assert!(
            !agents.refuse(Error::UnexpectedStatus(503)),
            "only 400 or 404"
        );
        assert!(agents.refuse(Error::NotFound));
        assert!(agents.subscribed().is_empty());
        assert!(!agents.refuse(Error::NotFound), "nothing left to refuse");
        assert!(agents.news("example-lyra", 0).unwrap().refused);
        let many: Vec<String> = (0..10).map(|n| format!("example-{n}")).collect();
        assert!(agents.set(many.iter().cloned().collect()));
        assert_eq!(agents.subscribed().len(), MAX_AGENTS);
        assert!(agents.set(BTreeSet::new()));
        assert!(agents.news("example-0", 0).is_none());
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
}
