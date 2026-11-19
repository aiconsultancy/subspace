//! The issue stream runs on a background thread. The fallback poll is
//! independent, so a slow connection cannot hold up updates to the open issue.
mod catalogue;
mod flight;
mod hints;
mod pace;
mod session;

pub(super) use catalogue::ProjectRefresh;
pub(super) use flight::{LiveReads, ReadKey};
pub(super) use hints::{reread_issue, reread_list};

use super::{ReadPriority, Shared, lock, worker::Task};
use crate::live::{self, LiveError};
use pace::{Backoff, POLL};
use session::Session;
use std::{
    collections::BTreeSet,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

/// How the board hears about changes made elsewhere.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LiveStatus {
    /// Waiting for authentication or for the scoped stream to be ready.
    #[default]
    Connecting,
    /// Hints arrive from the issue stream.
    Live,
    /// The connection dropped; trying again with backoff
    /// while the open issue is read every 30 s.
    Reconnecting,
    /// The stream is unavailable; the open issue is read every 30 s.
    Polling,
}

/// How often the workers check for a stop, a scope change or a due read.
const SLICE: Duration = Duration::from_millis(100);
/// Let rapid view changes settle before opening another stream.
const SCOPE_SETTLE: Duration = Duration::from_millis(200);

pub(super) fn start(shared: &Arc<Shared>) {
    if !shared.client.live {
        return;
    }
    let weak = Arc::downgrade(shared);
    let _ = std::thread::Builder::new()
        .name("subspace-live".into())
        .spawn(move || start_stream(&weak));
    let weak = Arc::downgrade(shared);
    let _ = std::thread::Builder::new()
        .name("subspace-live-poll".into())
        .spawn(move || poll(&weak));
}

fn start_stream(weak: &Weak<Shared>) {
    let Some(client) = weak.upgrade().map(|shared| shared.client.clone()) else {
        return;
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        set_status(weak, LiveStatus::Polling, None);
        return;
    };
    runtime.block_on(run(weak, &client));
}

async fn run(weak: &Weak<Shared>, client: &crate::Client) {
    let Ok(http) = live::http_client() else {
        set_status(weak, LiveStatus::Polling, None);
        return;
    };
    let mut backoff = Backoff::default();
    while let Some(scope) = settled_scope(weak).await {
        let mut session = Session::new(scope.generation);
        let result = tokio::select! {
            biased;
            () = wait_for_scope_change(weak, &scope) => {
                backoff.reset();
                if !set_status(weak, LiveStatus::Connecting, None) {
                    return;
                }
                continue;
            }
            result = session.run(weak, &http, client, &scope.projects) => result,
        };
        let Err(error) = result else {
            return;
        };
        if session.stable() {
            backoff.reset();
        }
        if !set_status(weak, retry_status(error), Some(scope.generation)) {
            return;
        }
        if error == LiveError::Unavailable {
            catalogue::request(weak, scope.generation);
        }
        let delay = backoff.next(rand::random_range(0.0..=1.0));
        tokio::select! {
            () = wait_for_scope_change(weak, &scope) => {
                backoff.reset();
                if !set_status(weak, LiveStatus::Connecting, None) {
                    return;
                }
            }
            () = tokio::time::sleep(delay) => {}
        }
    }
}

fn retry_status(error: LiveError) -> LiveStatus {
    match error {
        LiveError::Unavailable | LiveError::Refused => LiveStatus::Polling,
        _ => LiveStatus::Reconnecting,
    }
}

#[derive(PartialEq, Eq)]
struct StreamScope {
    generation: super::ScopeGeneration,
    projects: BTreeSet<String>,
}

/// An empty project set means the current account has no view to follow.
fn scope(weak: &Weak<Shared>) -> Option<StreamScope> {
    let shared = weak.upgrade()?;
    let state = lock(&shared.state);
    if !state.is_running() {
        return None;
    }
    let projects = state
        .snapshot
        .me
        .as_ref()
        .map_or_else(BTreeSet::new, |_| hints::wanted(&state));
    Some(StreamScope {
        generation: state.snapshot.generation,
        projects,
    })
}

async fn settled_scope(weak: &Weak<Shared>) -> Option<StreamScope> {
    loop {
        let before = scope(weak)?;
        tokio::time::sleep(SCOPE_SETTLE).await;
        if !before.projects.is_empty() && scope(weak)? == before {
            return Some(before);
        }
    }
}

async fn wait_for_scope_change(weak: &Weak<Shared>, subscribed: &StreamScope) {
    while scope(weak).as_ref() == Some(subscribed) {
        tokio::time::sleep(SLICE).await;
    }
}

/// Sets the current account's status; false once the hub stopped or dropped.
fn set_status(
    weak: &Weak<Shared>,
    status: LiveStatus,
    generation: Option<super::ScopeGeneration>,
) -> bool {
    let Some(shared) = weak.upgrade() else {
        return false;
    };
    let mut state = lock(&shared.state);
    if !state.is_running() {
        return false;
    }
    if generation.is_some_and(|generation| !state.current(generation)) {
        return true;
    }
    if state.snapshot.live != status {
        state.snapshot.live = status;
        state.changed();
    }
    true
}

/// Reads the open issue every `POLL` until the stream is live, independent
/// of connection setup, readiness and backoff.
fn poll(weak: &Weak<Shared>) {
    let mut polled = Instant::now();
    loop {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let mut state = lock(&shared.state);
        if !state.is_running() {
            return;
        }
        if state.snapshot.live == LiveStatus::Live {
            polled = Instant::now();
        } else if polled.elapsed() >= POLL {
            polled = Instant::now();
            hints::reread_issue(&shared, &mut state);
        }
        drop(state);
        drop(shared);
        std::thread::sleep(SLICE);
    }
}

/// Re-reads are queued as visible reads; they share the reader pacing.
fn enqueue(shared: &Shared, state: &mut super::State, task: Task) {
    shared.enqueue(state, task, ReadPriority::Visible);
}
