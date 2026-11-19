//! Finds a Copilot runtime that stopped answering. The watchdog is its own
//! task, so it neither waits for nor blocks the actor loop: it still runs
//! while the actor awaits a hung RPC. The chats publish their
//! [`fence::Activity`]; when a busy chat has had no SDK event for
//! `stall_after`, the watchdog pings the runtime through its own client
//! handle. A ping that fails or does not answer within `ping_timeout` ends
//! every turn it saw stalled, if that turn still runs, with a "not
//! responding" failure straight to the event queue. An answered ping counts
//! as a sign of life, so the next ping waits another `stall_after`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

use super::{Client, EventQueue, SessionSetup, lock};
use crate::failure::{FailureCategory, NOT_RESPONDING, TurnFailure};

#[path = "watchdog/fence.rs"]
pub(super) mod fence;

pub(super) use fence::Activities;

/// How long the watchdog waits, injectable so tests run in milliseconds.
#[derive(Clone, Copy, Debug)]
pub(super) struct Timing {
    /// How long a running turn may go without an SDK event before a ping.
    pub(super) stall_after: Duration,
    /// How long the runtime has to answer the ping.
    pub(super) ping_timeout: Duration,
    /// How often the watchdog looks for a stalled turn.
    pub(super) check_every: Duration,
}

pub(super) const TIMING: Timing = Timing {
    stall_after: Duration::from_secs(90),
    ping_timeout: Duration::from_secs(15),
    check_every: Duration::from_secs(15),
};

/// What the watchdog task works with.
pub(super) struct Watch {
    pub(super) client: Client,
    pub(super) activities: Activities,
    pub(super) events: Arc<EventQueue>,
    pub(super) runtime_version: Option<String>,
    pub(super) timing: Timing,
}

/// The running watchdog. Dropping it stops the task and releases its client.
pub(super) struct Watchdog(JoinHandle<()>);

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Starts the watchdog of a runtime's chats with the production timing.
pub(super) fn start(client: Client, setup: &SessionSetup, events: &Arc<EventQueue>) -> Watchdog {
    spawn(Watch {
        client,
        activities: setup.activity.clone(),
        events: events.clone(),
        runtime_version: setup.runtime_version.clone(),
        timing: TIMING,
    })
}

pub(super) fn spawn(watch: Watch) -> Watchdog {
    Watchdog(tokio::spawn(run(watch)))
}

async fn run(watch: Watch) {
    let mut interval = tokio::time::interval(watch.timing.check_every);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        check(&watch).await;
    }
}

/// Pings the runtime when a running turn went silent.
async fn check(watch: &Watch) {
    let stall_after = watch.timing.stall_after;
    let observed = fence::observe(&lock(&watch.activities), Instant::now(), stall_after);
    if observed.is_empty() {
        return;
    }
    let answered = matches!(
        tokio::time::timeout(watch.timing.ping_timeout, watch.client.ping(None)).await,
        Ok(Ok(_))
    );
    resolve(watch, &observed, answered).await;
}

/// After the ping: a sign of life, or the end of every observed turn that
/// still runs and is still silent.
async fn resolve(watch: &Watch, observed: &[(crate::ChatId, u64)], answered: bool) {
    let now = Instant::now();
    if answered {
        fence::mark_alive(&mut lock(&watch.activities), observed, now);
        return;
    }
    let ended = fence::expire(
        &mut lock(&watch.activities),
        observed,
        now,
        watch.timing.stall_after,
    );
    for (chat, turn) in ended {
        let failure = TurnFailure::stopped(
            FailureCategory::NotResponding,
            NOT_RESPONDING,
            &turn,
            watch.runtime_version.as_deref(),
        );
        super::chat::end_reported(&watch.events, chat, &turn, failure).await;
    }
}

#[cfg(test)]
#[path = "watchdog/tests.rs"]
mod tests;
