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
use crate::backend::Feature;
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
    /// NETT, which updates only over its stream: the stream is not open, so
    /// changes made elsewhere show after a reload.
    Off,
}

/// How often the workers check for a stop, a scope change or a due read.
const SLICE: Duration = Duration::from_millis(100);
/// Let rapid view changes settle before opening another stream.
const SCOPE_SETTLE: Duration = Duration::from_millis(200);
/// The longest the hub waits for its catch-up reads before reconnecting.
const CATCH_UP: Duration = Duration::from_secs(15);

pub(super) fn start(shared: &Arc<Shared>) {
    if !shared.client.live {
        return;
    }
    let weak = Arc::downgrade(shared);
    let _ = std::thread::Builder::new()
        .name("subspace-live".into())
        .spawn(move || start_stream(&weak));
    // NETT updates only over its stream; Subspace API's open issue is also
    // read on a timer while its stream is down.
    if shared.client.service() == crate::Backend::Subspace {
        let weak = Arc::downgrade(shared);
        let _ = std::thread::Builder::new()
            .name("subspace-live-poll".into())
            .spawn(move || poll(&weak));
    }
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
    if client.service() == crate::Backend::Nett {
        run_nett(weak, client, &http).await;
    } else {
        run_subspace(weak, client, &http).await;
    }
}

async fn run_subspace(weak: &Weak<Shared>, client: &crate::Client, http: &reqwest::Client) {
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
            result = session.run(weak, http, client, &scope.projects) => result,
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

/// NETT updates only over its stream. It is opened where the organisation
/// serves it; a refused bearer is renewed once; a 403 reads again what the
/// person may see, and a second one in a row stops the stream until the
/// followed scope or the capability list changes; a 404 stops it for the
/// rest of the hub's session. After any disconnect from a live stream the
/// hub catches up through REST, and waits for those reads, before it
/// reconnects. A server's `Retry-After`, up to five minutes, holds however
/// the followed teams change meanwhile.
async fn run_nett(weak: &Weak<Shared>, client: &crate::Client, http: &reqwest::Client) {
    let mut backoff = Backoff::default();
    let mut renewed = false;
    let mut forbidden = 0_u32;
    let mut stopped: Option<super::ScopeGeneration> = None;
    let mut not_before: Option<Instant> = None;
    while let Some(scope) = settled_scope(weak).await {
        if stopped == Some(scope.generation) {
            if !set_status(weak, LiveStatus::Off, Some(scope.generation)) {
                return;
            }
            wait_for_generation_change(weak, scope.generation).await;
            continue;
        }
        if let Some(wait) = not_before.and_then(|at| at.checked_duration_since(Instant::now())) {
            // The server's wait is kept even when the followed teams change.
            tokio::select! {
                () = wait_for_generation_change(weak, scope.generation) => {}
                () = tokio::time::sleep(wait) => {}
            }
            continue;
        }
        match client.nett_serves(Feature::Updates) {
            Ok(true) => {}
            Ok(false) => {
                if !set_status(weak, LiveStatus::Off, Some(scope.generation)) {
                    return;
                }
                let epoch = scope_epoch(weak);
                wait_for_scope_or_epoch_change(weak, &scope, epoch).await;
                continue;
            }
            // The list could not be read: try again later, not "never".
            Err(_) => {
                if !set_status(weak, LiveStatus::Reconnecting, Some(scope.generation)) {
                    return;
                }
                let delay = backoff.next(rand::random_range(0.0..=1.0));
                tokio::select! {
                    () = wait_for_scope_change(weak, &scope) => backoff.reset(),
                    () = tokio::time::sleep(delay) => {}
                }
                continue;
            }
        }
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
            result = session.run(weak, http, client, &scope.projects) => result,
        };
        let Err(error) = result else {
            return;
        };
        if session.stable() {
            backoff.reset();
        }
        if session.went_live() {
            renewed = false;
            forbidden = 0;
        }
        if error != LiveError::Forbidden {
            forbidden = 0;
        }
        match error {
            LiveError::Unauthorized if !renewed => {
                // The token may have been revoked before it expired: renew
                // it once and reconnect straight away.
                renewed = true;
                let renewal = tokio::task::spawn_blocking(crate::native::renew_token).await;
                // Reads refused meanwhile are read again with the new token.
                if renewal.unwrap_or(false)
                    && let Some(shared) = weak.upgrade()
                {
                    super::renewal::renewed(&shared, scope.generation);
                }
                if !set_status(weak, LiveStatus::Reconnecting, Some(scope.generation)) {
                    return;
                }
                continue;
            }
            LiveError::Unauthorized | LiveError::Unavailable => {
                // A renewed token refused again, or a team gone: no stream
                // until the account or organisation changes.
                if error == LiveError::Unavailable {
                    catalogue::request(weak, scope.generation);
                }
                refresh_scope(weak, client);
                stopped = Some(scope.generation);
                if !set_status(weak, LiveStatus::Off, Some(scope.generation)) {
                    return;
                }
                continue;
            }
            LiveError::Forbidden => {
                // Access changed: read again what the person may see. A
                // second refusal in a row stops reconnecting, so a lasting
                // refusal never turns into polling.
                forbidden += 1;
                catch_up(weak, scope.generation).await;
                refresh_scope(weak, client);
                if forbidden >= 2 {
                    if !set_status(weak, LiveStatus::Off, Some(scope.generation)) {
                        return;
                    }
                    let epoch = scope_epoch(weak);
                    wait_for_scope_or_epoch_change(weak, &scope, epoch).await;
                    forbidden = 0;
                    continue;
                }
            }
            // Any end of a stream that was live, a clean close or a broken
            // connection: hints may have been missed, so catch up first.
            _ if session.went_live() => {
                catch_up(weak, scope.generation).await;
                refresh_scope(weak, client);
            }
            _ => {}
        }
        if !set_status(weak, LiveStatus::Reconnecting, Some(scope.generation)) {
            return;
        }
        let floor = match error {
            LiveError::Busy(Some(seconds)) => {
                // Honoured as every NETT wait is: never shortened, for at
                // most five minutes.
                let floor = crate::nett::flow::pause(0, Some(seconds), 0.0);
                not_before = Some(crate::nett::flow::deadline(Instant::now(), floor));
                floor
            }
            _ => Duration::ZERO,
        };
        let delay = backoff.next(rand::random_range(0.0..=1.0)).max(floor);
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

fn scope_epoch(weak: &Weak<Shared>) -> u64 {
    weak.upgrade()
        .map_or(0, |shared| lock(&shared.state).snapshot.scope_epoch)
}

/// Until the followed scope changes or the capability list is read again.
async fn wait_for_scope_or_epoch_change(weak: &Weak<Shared>, scope: &StreamScope, epoch: u64) {
    while self::scope(weak).as_ref() == Some(scope) && scope_epoch(weak) == epoch {
        tokio::time::sleep(SLICE).await;
    }
}

/// Until the hub's scope generation changes or the hub stops.
async fn wait_for_generation_change(weak: &Weak<Shared>, generation: super::ScopeGeneration) {
    while self::scope(weak).is_some_and(|scope| scope.generation == generation) {
        tokio::time::sleep(SLICE).await;
    }
}

/// Reads the open issue and the list head again after a close, and waits,
/// up to a bound, until those reads are back.
async fn catch_up(weak: &Weak<Shared>, generation: super::ScopeGeneration) {
    {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let mut state = lock(&shared.state);
        if !state.current(generation) {
            return;
        }
        hints::reread_issue(&shared, &mut state);
        hints::reread_list(&shared, &mut state);
    }
    let deadline = Instant::now() + CATCH_UP;
    while Instant::now() < deadline {
        let pending = weak.upgrade().is_some_and(|shared| {
            let state = lock(&shared.state);
            state.current(generation)
                && (state.live_reads.issue.pending() || state.live_reads.list.pending())
        });
        if !pending {
            return;
        }
        tokio::time::sleep(SLICE).await;
    }
}

/// Reads the organisation's capability list again and tells the Spaces to
/// read their initiatives again.
fn refresh_scope(weak: &Weak<Shared>, client: &crate::Client) {
    let _ = client.nett_refresh_served();
    let Some(shared) = weak.upgrade() else {
        return;
    };
    let mut state = lock(&shared.state);
    if state.is_running() {
        state.snapshot.scope_epoch = state.snapshot.scope_epoch.wrapping_add(1);
        state.changed();
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
    let mut projects = state
        .snapshot
        .me
        .as_ref()
        .map_or_else(BTreeSet::new, |_| hints::wanted(&state));
    if shared.client.service() == crate::Backend::Nett {
        projects = bounded(projects, &state);
    }
    Some(StreamScope {
        generation: state.snapshot.generation,
        projects,
    })
}

/// NETT accepts at most 64 team keys on one stream. Past that, the open
/// issue's team and the chosen team come first, then the rest in order.
fn bounded(projects: BTreeSet<String>, state: &super::State) -> BTreeSet<String> {
    if projects.len() <= live::MAX_PROJECTS {
        return projects;
    }
    let open = state
        .snapshot
        .issue
        .as_ref()
        .and_then(|issue| issue.detail.as_ref())
        .map(|detail| detail.fields.project_key.clone());
    let first: Vec<String> = open
        .into_iter()
        .chain(state.snapshot.selected_project.clone())
        .filter(|key| projects.contains(key))
        .collect();
    first
        .iter()
        .cloned()
        .chain(projects.into_iter().filter(|key| !first.contains(key)))
        .take(live::MAX_PROJECTS)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::bounded;
    use std::collections::BTreeSet;

    #[test]
    fn a_nett_stream_follows_at_most_64_teams_keeping_the_chosen_one() {
        let mut state = crate::hub::State::new("http://127.0.0.1:9/api", "NETT");
        state.snapshot.selected_project = Some("ZZ99".into());
        let mut keys: BTreeSet<String> = (0..70).map(|n| format!("T{n:02}")).collect();
        keys.insert("ZZ99".into());
        let followed = bounded(keys, &state);
        assert_eq!(followed.len(), 64);
        assert!(
            followed.contains("ZZ99"),
            "the chosen team is always followed"
        );
        let few: BTreeSet<String> = ["OPS".into(), "WEB".into()].into();
        assert_eq!(bounded(few.clone(), &state), few);
    }
}
