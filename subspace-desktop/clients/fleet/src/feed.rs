//! Following one fleet on a background thread: its event stream, reconnected
//! with backoff, and while Stargate refuses the stream (it serves at most
//! eight), the overview and ship details read on a timer, trying the stream
//! again now and then. A refused token or network stops it until the token
//! changes, which starts a new feed.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use crate::activity::ActivityNews;
use crate::events::FleetEvent;
use crate::model::{ActivityEvent, FleetOverview, ShipDetail};
use crate::stream::{self, StreamEnd};
use crate::{Error, FleetClient};

mod agents;
mod link;
mod pace;

use agents::Agents;

pub use link::Link;
pub(crate) use pace::Pace;
use pace::{Backoff, MIN_JITTER};

/// The most activity lines a snapshot keeps.
pub const MAX_ACTIVITY: usize = 50;
/// Answers that no retry can change, and the link each leaves.
const LASTING: [(Error, Link); 3] = [
    (Error::TokenRefused, Link::TokenRefused),
    (Error::Forbidden, Link::Forbidden),
    (Error::InvalidAddress, Link::Unreachable),
];

/// What a feed knows of its fleet.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct FleetSnapshot {
    pub link: Link,
    /// The latest overview, once one has arrived.
    pub overview: Option<FleetOverview>,
    /// The latest detail of each ship, by ship ID.
    pub ships: BTreeMap<String, ShipDetail>,
    /// The latest activity, oldest first, at most [`MAX_ACTIVITY`].
    pub activity: VecDeque<ActivityEvent>,
}

#[derive(Default)]
struct Shared {
    state: Mutex<FleetSnapshot>,
    revision: AtomicU64,
    stopped: AtomicBool,
    wake: tokio::sync::Notify,
    /// The officers watched and their kept `agent-activity` events.
    agents: Mutex<Agents>,
    /// Told when the officers the stream subscribes to change.
    retarget: tokio::sync::Notify,
}

/// One fleet followed in the background until dropped.
pub struct Feed(Arc<Shared>);

impl Feed {
    /// Follows `client`'s fleet on a new thread.
    pub fn start(client: FleetClient) -> Self {
        Self::start_with(client, Pace::default())
    }

    pub(crate) fn start_with(client: FleetClient, pace: Pace) -> Self {
        let shared = Arc::new(Shared::default());
        let thread = Arc::clone(&shared);
        let started = std::thread::Builder::new()
            .name("fleet-feed".into())
            .spawn(move || run_thread(&thread, &client, pace));
        if started.is_err() {
            set_link(&shared, Link::Unreachable);
        }
        Self(shared)
    }

    /// A copy of what the feed knows now.
    pub fn snapshot(&self) -> FleetSnapshot {
        lock(&self.0.state).clone()
    }

    /// Changes with every update, so a caller can tell when to read again.
    pub fn revision(&self) -> u64 {
        self.0.revision.load(Ordering::Acquire)
    }

    pub fn link(&self) -> Link {
        lock(&self.0.state).link
    }

    /// Watches exactly these officers' turn activity, subscribing the
    /// stream to the first eight and telling readers of the rest that no
    /// stream carries them, and opens the stream again with the new query
    /// when that changes.
    pub(crate) fn set_agents(&self, agents: std::collections::BTreeSet<String>) {
        if lock(&self.0.agents).set(agents) {
            self.0.retarget.notify_one();
        }
    }

    /// What a reader at position `after` has not taken of `agent`'s
    /// events, while the feed watches `agent`.
    pub(crate) fn activity_news(&self, agent: &str, after: u64) -> Option<ActivityNews> {
        lock(&self.0.agents).news(agent, after)
    }
}

#[cfg(test)]
impl Feed {
    /// A feed with no thread behind it: its link changes only when a test
    /// changes it ([`Self::set_link_for_test`]).
    pub(crate) fn detached() -> Self {
        Self(Arc::new(Shared::default()))
    }

    /// Changes the link as the feed's thread would, off the app's executor.
    pub(crate) fn set_link_for_test(&self, link: Link) {
        set_link(&self.0, link);
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.0.stopped.store(true, Ordering::Release);
        self.0.wake.notify_one();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn run_thread(shared: &Arc<Shared>, client: &FleetClient, pace: Pace) {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(run(shared, client, pace)),
        Err(_) => set_link(shared, Link::Unreachable),
    }
}

async fn run(shared: &Arc<Shared>, client: &FleetClient, pace: Pace) {
    let Ok(http) = stream::http_client() else {
        set_link(shared, Link::Unreachable);
        return;
    };
    let mut backoff = Backoff::new(pace);
    loop {
        let Some((end, stable, attempted)) =
            until_stopped(shared, follow(shared, &http, client, pace)).await
        else {
            return;
        };
        if stable {
            backoff.reset();
        }
        lock(&shared.agents).disconnected();
        bump(shared);
        let next = match end {
            StreamEnd::Retarget => Some(()),
            StreamEnd::Refused(error) if lock(&shared.agents).refuse(error, &attempted) => Some(()),
            StreamEnd::Refused(error) => refused(shared, client, pace, error).await,
            StreamEnd::Network => wait_to_retry(shared, &mut backoff, Link::Unreachable).await,
            StreamEnd::Protocol | StreamEnd::Closed => {
                wait_to_retry(shared, &mut backoff, Link::Reconnecting).await
            }
        };
        if next.is_none() {
            return;
        }
    }
}

/// Streams until the stream ends; whether it stayed up long enough to
/// count as stable, and the officers it asked for, which a refusal is
/// about: the set may have changed while it connected.
async fn follow(
    shared: &Shared,
    http: &reqwest::Client,
    client: &FleetClient,
    pace: Pace,
) -> (StreamEnd, bool, Vec<String>) {
    let subscribed = lock(&shared.agents).subscribed();
    let mut events = match stream::connect(http, client, &subscribed).await {
        Ok(events) => events,
        Err(end) => return (end, false, subscribed),
    };
    lock(&shared.agents).connected(&subscribed);
    set_link(shared, Link::Live);
    bump(shared);
    let opened = Instant::now();
    loop {
        let next = tokio::select! {
            next = events.next() => next,
            () = shared.retarget.notified() => {
                if lock(&shared.agents).subscribed() == subscribed {
                    continue;
                }
                Err(StreamEnd::Retarget)
            }
        };
        match next {
            Ok(event) => apply(shared, event),
            Err(end) => return (end, opened.elapsed() >= pace.stable, subscribed),
        }
    }
}

/// After Stargate refused the stream: stop for a refused token or network,
/// otherwise read on a timer for a while. `None` once stopped.
async fn refused(shared: &Shared, client: &FleetClient, pace: Pace, error: Error) -> Option<()> {
    if let Some(link) = blocked(error) {
        set_link(shared, link);
        stopped(shared).await;
        return None;
    }
    match until_stopped(shared, poll(shared, client, pace)).await? {
        Ok(()) => Some(()),
        Err(link) => {
            set_link(shared, link);
            stopped(shared).await;
            None
        }
    }
}

/// The link for an answer that no retry can change.
fn blocked(error: Error) -> Option<Link> {
    LASTING
        .iter()
        .find(|(lasting, _)| *lasting == error)
        .map(|(_, link)| *link)
}

/// Reads the fleet every `pace.poll` for `pace.stream_retry`. Ends early
/// with the link to keep when the token or network is refused.
async fn poll(shared: &Shared, client: &FleetClient, pace: Pace) -> Result<(), Link> {
    let until = Instant::now() + pace.stream_retry;
    loop {
        match read_all(client.clone()).await {
            Ok((overview, ships)) => replace(shared, overview, ships),
            Err(error) => {
                if let Some(link) = blocked(error) {
                    return Err(link);
                }
                set_link(shared, Link::Unreachable);
            }
        }
        if Instant::now() + pace.poll >= until {
            return Ok(());
        }
        tokio::time::sleep(pace.poll).await;
    }
}

/// The overview and every ship's detail, read off the runtime's thread.
/// A ship whose detail cannot be read keeps its last one.
async fn read_all(client: FleetClient) -> Result<(FleetOverview, Vec<ShipDetail>), Error> {
    tokio::task::spawn_blocking(move || {
        let overview = client.fleet()?;
        let ships = overview
            .ships
            .iter()
            .filter_map(|ship| client.ship(&ship.id).ok())
            .collect();
        Ok((overview, ships))
    })
    .await
    .map_err(|_| Error::Unreachable)?
}

async fn wait_to_retry(shared: &Shared, backoff: &mut Backoff, link: Link) -> Option<()> {
    set_link(shared, link);
    let delay = backoff.next(rand::random_range(MIN_JITTER..=1.0));
    until_stopped(shared, tokio::time::sleep(delay)).await
}

/// `future`'s output, or `None` once the feed is dropped.
async fn until_stopped<T>(shared: &Shared, future: impl Future<Output = T>) -> Option<T> {
    tokio::select! {
        biased;
        () = stopped(shared) => None,
        value = future => Some(value),
    }
}

async fn stopped(shared: &Shared) {
    while !shared.stopped.load(Ordering::Acquire) {
        shared.wake.notified().await;
    }
}

fn set_link(shared: &Shared, link: Link) {
    update(shared, |state| {
        let changed = state.link != link;
        state.link = link;
        changed
    });
}

/// Changes the snapshot with `change`, which says whether it changed it.
fn update(shared: &Shared, change: impl FnOnce(&mut FleetSnapshot) -> bool) {
    if change(&mut lock(&shared.state)) {
        shared.revision.fetch_add(1, Ordering::AcqRel);
    }
}

fn apply(shared: &Shared, event: FleetEvent) {
    match event {
        FleetEvent::AgentActivity(activity) => {
            if lock(&shared.agents).push(*activity) {
                bump(shared);
            }
        }
        event => update(shared, |state| apply_event(state, event)),
    }
}

/// Tells readers there is news outside the snapshot.
fn bump(shared: &Shared) {
    shared.revision.fetch_add(1, Ordering::AcqRel);
}

fn replace(shared: &Shared, overview: FleetOverview, ships: Vec<ShipDetail>) {
    update(shared, |state| {
        state.link = Link::Polling;
        state.ships = ships
            .into_iter()
            .map(|ship| (ship.ship.id.clone(), ship))
            .collect();
        state.overview = Some(overview);
        true
    });
}

/// Applies one event; whether anything visible changed.
pub(crate) fn apply_event(state: &mut FleetSnapshot, event: FleetEvent) -> bool {
    match event {
        FleetEvent::Fleet(overview) => {
            // A ship that left the roster takes its detail with it.
            state.ships.retain(|id, _| overview.ship(id).is_some());
            state.overview = Some(*overview);
        }
        FleetEvent::Ship(detail) => {
            state.ships.insert(detail.ship.id.clone(), *detail);
        }
        FleetEvent::ShipSampled {
            id,
            detail_updated_at,
        } => {
            let Some(detail) = state.ships.get_mut(&id) else {
                return false;
            };
            detail.detail_updated_at = Some(detail_updated_at);
        }
        FleetEvent::Host(host) => {
            let Some(overview) = state.overview.as_mut() else {
                return false;
            };
            overview.host = Some(*host);
        }
        FleetEvent::Activity(activity) => {
            state.activity.push_back(*activity);
            while state.activity.len() > MAX_ACTIVITY {
                state.activity.pop_front();
            }
        }
        FleetEvent::Heartbeat { .. } | FleetEvent::AgentActivity(_) => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{FleetSnapshot, MAX_ACTIVITY, apply_event};
    use crate::events::FleetEvent;
    use crate::model::{ActivityEvent, FleetOverview, ShipDetail};

    fn overview(ships: &[&str]) -> FleetOverview {
        serde_json::from_value(serde_json::json!({
            "name": "Example",
            "ships": ships.iter().map(|id| serde_json::json!({"id": id})).collect::<Vec<_>>()
        }))
        .unwrap()
    }

    fn detail(id: &str) -> ShipDetail {
        serde_json::from_value(serde_json::json!({"id": id, "name": id})).unwrap()
    }

    #[test]
    fn events_update_the_snapshot_and_a_new_roster_drops_departed_ships() {
        let mut state = FleetSnapshot::default();
        assert!(!apply_event(&mut state, FleetEvent::Heartbeat { at: 1 }));
        assert!(apply_event(
            &mut state,
            FleetEvent::Fleet(Box::new(overview(&["a", "b"])))
        ));
        assert!(apply_event(
            &mut state,
            FleetEvent::Ship(Box::new(detail("a")))
        ));
        assert!(apply_event(
            &mut state,
            FleetEvent::Ship(Box::new(detail("b")))
        ));
        let sampled = FleetEvent::ShipSampled {
            id: "a".into(),
            detail_updated_at: 42,
        };
        assert!(apply_event(&mut state, sampled));
        assert_eq!(state.ships["a"].detail_updated_at, Some(42));
        let unknown = FleetEvent::ShipSampled {
            id: "z".into(),
            detail_updated_at: 1,
        };
        assert!(!apply_event(&mut state, unknown));
        assert!(apply_event(
            &mut state,
            FleetEvent::Fleet(Box::new(overview(&["b"])))
        ));
        assert_eq!(state.ships.keys().collect::<Vec<_>>(), ["b"]);
        for at in 0..60 {
            let line = ActivityEvent {
                at,
                ..ActivityEvent::default()
            };
            apply_event(&mut state, FleetEvent::Activity(Box::new(line)));
        }
        assert_eq!(state.activity.len(), MAX_ACTIVITY);
        assert_eq!(state.activity.front().map(|line| line.at), Some(10));
    }
}
