//! Following fleets: one feed per followed fleet, shared by every handle,
//! started when a token is saved for the fleet's address and stopped when
//! the last handle goes or the token or address changes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::Context;
use subspace_framework::core::Secret;

use super::FleetApi;
use crate::feed::Feed;
use crate::{FleetAddress, FleetClient};

/// How often followed fleets are checked for news.
const TICK: Duration = Duration::from_millis(250);

/// Keeps a fleet followed while held. Dropping the last handle for a fleet
/// lets its stream close.
#[derive(Clone)]
pub struct FleetFollow {
    fleet: String,
    _holder: Arc<()>,
}

impl FleetFollow {
    pub fn fleet(&self) -> &str {
        &self.fleet
    }
}

pub(super) struct Followed {
    pub(super) feed: Feed,
    /// The client the feed was started with: a new token or address
    /// starts a new feed.
    address: FleetAddress,
    token: Secret,
    revision: u64,
}

impl Followed {
    fn start(client: FleetClient) -> Self {
        Self {
            address: client.address().clone(),
            token: client.token().clone(),
            feed: Feed::start(client),
            revision: u64::MAX,
        }
    }

    fn serves(&self, client: &FleetClient) -> bool {
        client.address() == &self.address && client.token() == &self.token
    }
}

impl FleetApi {
    /// Follows `fleet` while the handle is held: its event stream opens as
    /// soon as a token is saved for its address. Every handle for a fleet
    /// shares one stream.
    pub fn follow(&mut self, fleet: &str, cx: &mut Context<Self>) -> FleetFollow {
        let holder = self
            .interest
            .entry(fleet.to_owned())
            .or_insert_with(|| Arc::new(()))
            .clone();
        self.reconcile(cx);
        FleetFollow {
            fleet: fleet.to_owned(),
            _holder: holder,
        }
    }

    /// Starts a feed for each followed fleet with a ready client, and stops
    /// feeds nothing follows any more or whose token or address changed.
    pub(super) fn reconcile(&mut self, cx: &mut Context<Self>) {
        self.interest
            .retain(|_, holder| Arc::strong_count(holder) > 1);
        let wanted: BTreeMap<String, FleetClient> = self
            .interest
            .keys()
            .filter_map(|fleet| Some((fleet.clone(), self.client(fleet).ok()?)))
            .collect();
        self.feeds.retain(|fleet, followed| {
            wanted
                .get(fleet)
                .is_some_and(|client| followed.serves(client))
        });
        for (fleet, client) in wanted {
            self.feeds
                .entry(fleet)
                .or_insert_with(|| Followed::start(client));
        }
        self.keep_ticking(cx);
    }

    /// Checks followed fleets on a timer while any are followed.
    fn keep_ticking(&mut self, cx: &mut Context<Self>) {
        if self.ticking || self.interest.is_empty() {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                match this.update(cx, Self::tick) {
                    Ok(true) => {}
                    _ => return,
                }
            }
        })
        .detach();
    }

    /// One check of the followed fleets; whether to keep checking.
    fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        let interested = self.interest.len();
        self.interest
            .retain(|_, holder| Arc::strong_count(holder) > 1);
        self.feeds
            .retain(|fleet, _| self.interest.contains_key(fleet));
        let mut news = self.interest.len() != interested;
        for followed in self.feeds.values_mut() {
            let revision = followed.feed.revision();
            news |= std::mem::replace(&mut followed.revision, revision) != revision;
        }
        if news {
            self.revision = self.revision.wrapping_add(1);
            cx.notify();
        }
        self.ticking = !self.interest.is_empty();
        self.ticking
    }
}
