//! Watching an officer's turns: while a handle is held, the fleet's one
//! event stream subscribes to the officer's `agent-activity` events with
//! `?agent=`, and drops the parameter once the last handle goes. No
//! second stream is ever opened for it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gpui_kit::Context;

use super::{FleetApi, FleetFollow};
use crate::activity::ActivityNews;

/// Keeps an officer's turn activity subscribed, and its fleet followed,
/// while held.
#[derive(Clone)]
pub struct ActivityWatch {
    follow: FleetFollow,
    agent: String,
    _holder: Arc<()>,
}

impl ActivityWatch {
    pub fn fleet(&self) -> &str {
        self.follow.fleet()
    }

    /// The officer's registration ID.
    pub fn agent(&self) -> &str {
        &self.agent
    }
}

impl FleetApi {
    /// Follows `fleet` and subscribes its stream to the turn activity of
    /// the officer with registration ID `agent` while the handle is held.
    /// Read what arrives with [`Self::activity_news`].
    pub fn watch_activity(
        &mut self,
        fleet: &str,
        agent: &str,
        cx: &mut Context<Self>,
    ) -> ActivityWatch {
        let holder = self
            .watched
            .entry((fleet.to_owned(), agent.to_owned()))
            .or_insert_with(|| Arc::new(()))
            .clone();
        let follow = self.follow(fleet, cx);
        self.sync_agents();
        ActivityWatch {
            follow,
            agent: agent.to_owned(),
            _holder: holder,
        }
    }

    /// The officer's events since position `after`, while a handle watches
    /// it and its fleet's stream runs ([`crate::activity::ActivityJoin`]).
    pub fn activity_news(&self, fleet: &str, agent: &str, after: u64) -> Option<ActivityNews> {
        self.feeds.get(fleet)?.feed.activity_news(agent, after)
    }

    /// Lets go of officers nothing watches and tells each feed which
    /// officers its stream subscribes to.
    pub(super) fn sync_agents(&mut self) {
        self.watched
            .retain(|_, holder| Arc::strong_count(holder) > 1);
        let mut wanted: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        for (fleet, agent) in self.watched.keys() {
            wanted
                .entry(fleet.as_str())
                .or_default()
                .insert(agent.clone());
        }
        for (fleet, followed) in &self.feeds {
            followed
                .feed
                .set_agents(wanted.remove(fleet.as_str()).unwrap_or_default());
        }
    }
}
