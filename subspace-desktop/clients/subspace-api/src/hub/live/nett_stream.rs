//! NETT updates only over its stream. It is opened where the organisation
//! serves it; a refused bearer is renewed once; a 403 reads again what the
//! person may see, and a second one in a row stops the stream until the
//! followed scope or the capability list changes; a 404 stops it for the
//! rest of the hub's session. After any disconnect from a live stream the
//! hub catches up through REST, and waits for those reads, before it
//! reconnects. A server's `Retry-After`, up to five minutes, holds however
//! the followed teams change meanwhile.
mod capabilities;

use super::pace::Backoff;
use super::session::Session;
use super::{
    LiveStatus, SLICE, StreamScope, catalogue, hints, set_status, settled_scope,
    wait_for_scope_change,
};
use crate::backend::Feature;
use crate::hub::{ScopeGeneration, Shared, State, lock};
use crate::live::{self, LiveError};
use capabilities::{
    recheck_capabilities, refresh_scope, scope_epoch, wait_for_scope_epoch_or_capabilities,
    wait_for_scope_or_epoch_change,
};
use std::collections::BTreeSet;
use std::sync::Weak;
use std::time::{Duration, Instant};

/// The longest the hub waits for its catch-up reads before reconnecting.
const CATCH_UP: Duration = Duration::from_secs(15);
/// Refusals in a row that stop reconnecting, so a lasting refusal never
/// turns into polling.
const MAX_REFUSALS: u32 = 2;

/// How a round of the stream loop ends.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    /// Follow the scope again.
    Again,
    /// Back off, then follow the scope again.
    Reconnect,
    /// The hub stopped or dropped.
    Stop,
}

impl Next {
    /// [`Next::Again`] while the hub runs, as [`set_status`] reports it.
    fn running(running: bool) -> Self {
        if running { Self::Again } else { Self::Stop }
    }
}

/// The stream's state across reconnects.
struct Stream<'a> {
    weak: &'a Weak<Shared>,
    client: &'a crate::Client,
    http: &'a reqwest::Client,
    backoff: Backoff,
    /// Whether the token was renewed since the stream was last live.
    renewed: bool,
    /// Refusals in a row.
    refusals: u32,
    /// The scope generation the stream stopped for, until it changes.
    stopped: Option<ScopeGeneration>,
    /// The end of a server's wait.
    not_before: Option<Instant>,
}

pub(super) async fn run(weak: &Weak<Shared>, client: &crate::Client, http: &reqwest::Client) {
    let mut stream = Stream {
        weak,
        client,
        http,
        backoff: Backoff::default(),
        renewed: false,
        refusals: 0,
        stopped: None,
        not_before: None,
    };
    while let Some(scope) = settled_scope(weak).await {
        if stream.round(&scope).await == Next::Stop {
            return;
        }
    }
}

impl Stream<'_> {
    /// One round: stay off, hold for the server's wait, or open the stream
    /// where the organisation serves it.
    async fn round(&mut self, scope: &StreamScope) -> Next {
        if self.stopped == Some(scope.generation) {
            return self.stay_off(scope.generation).await;
        }
        if let Some(wait) = self.held() {
            hold(self.weak, scope.generation, wait).await;
            return Next::Again;
        }
        match self.client.nett_serves(Feature::Updates) {
            Ok(true) => self.connect(scope).await,
            Ok(false) => self.unserved(scope).await,
            // The list could not be read: try again later, not "never".
            Err(_) => self.unreadable(scope).await,
        }
    }

    fn status(&self, status: LiveStatus, generation: Option<ScopeGeneration>) -> Next {
        Next::running(set_status(self.weak, status, generation))
    }

    /// Off until the followed scope changes.
    async fn stay_off(&self, generation: ScopeGeneration) -> Next {
        if self.status(LiveStatus::Off, Some(generation)) == Next::Stop {
            return Next::Stop;
        }
        wait_for_generation_change(self.weak, generation).await;
        Next::Again
    }

    /// What is left of the server's wait.
    fn held(&self) -> Option<Duration> {
        self.not_before
            .and_then(|at| at.checked_duration_since(Instant::now()))
    }

    /// NETT may enable the stream while the app runs: once the list is due
    /// to be read again, after its lifetime or an explicit reload, it is
    /// read, and the stream opened if the organisation now serves it.
    async fn unserved(&self, scope: &StreamScope) -> Next {
        if self.status(LiveStatus::Off, Some(scope.generation)) == Next::Stop {
            return Next::Stop;
        }
        let epoch = scope_epoch(self.weak);
        if wait_for_scope_epoch_or_capabilities(self.weak, scope, epoch, self.client).await {
            recheck_capabilities(self.weak, self.client);
        }
        Next::Again
    }

    async fn unreadable(&mut self, scope: &StreamScope) -> Next {
        if self.status(LiveStatus::Reconnecting, Some(scope.generation)) == Next::Stop {
            return Next::Stop;
        }
        let delay = self.backoff.next(rand::random_range(0.0..=1.0));
        tokio::select! {
            () = wait_for_scope_change(self.weak, scope) => self.backoff.reset(),
            () = tokio::time::sleep(delay) => {}
        }
        Next::Again
    }

    /// Opens the stream and, once it ends, decides what follows.
    async fn connect(&mut self, scope: &StreamScope) -> Next {
        let mut session = Session::new(scope.generation);
        let result = tokio::select! {
            biased;
            () = wait_for_scope_change(self.weak, scope) => {
                self.backoff.reset();
                return self.status(LiveStatus::Connecting, None);
            }
            result = session.run(self.weak, self.http, self.client, &scope.projects) => result,
        };
        let Err(error) = result else {
            return Next::Stop;
        };
        self.note(&session, error);
        match self.closed(scope, &session, error).await {
            Next::Reconnect => self.back_off(scope, error).await,
            next => next,
        }
    }

    /// What an ended session says about the backoff and the refusals.
    fn note(&mut self, session: &Session, error: LiveError) {
        if session.stable() {
            self.backoff.reset();
        }
        if session.went_live() {
            self.renewed = false;
            self.refusals = 0;
        }
        if error != LiveError::Forbidden {
            self.refusals = 0;
        }
    }

    /// What follows a stream that ended with `error`.
    async fn closed(&mut self, scope: &StreamScope, session: &Session, error: LiveError) -> Next {
        match error {
            LiveError::Unauthorized if !self.renewed => self.renew(scope.generation).await,
            LiveError::Unauthorized | LiveError::Unavailable => self.stop(scope.generation, error),
            LiveError::Forbidden => self.refused(scope).await,
            // Any end of a stream that was live, a clean close or a broken
            // connection: hints may have been missed, so catch up first.
            _ if session.went_live() => {
                catch_up(self.weak, scope.generation).await;
                refresh_scope(self.weak, self.client);
                Next::Reconnect
            }
            _ => Next::Reconnect,
        }
    }

    /// The token may have been revoked before it expired: renew it once and
    /// reconnect straight away. Reads refused meanwhile are read again with
    /// the new token.
    async fn renew(&mut self, generation: ScopeGeneration) -> Next {
        self.renewed = true;
        let renewal = tokio::task::spawn_blocking(crate::native::renew_token).await;
        if renewal.unwrap_or(false)
            && let Some(shared) = self.weak.upgrade()
        {
            crate::hub::renewal::renewed(&shared, generation);
        }
        self.status(LiveStatus::Reconnecting, Some(generation))
    }

    /// A renewed token refused again, or a team gone: no stream until the
    /// account or organisation changes.
    fn stop(&mut self, generation: ScopeGeneration, error: LiveError) -> Next {
        if error == LiveError::Unavailable {
            catalogue::request(self.weak, generation);
        }
        refresh_scope(self.weak, self.client);
        self.stopped = Some(generation);
        self.status(LiveStatus::Off, Some(generation))
    }

    /// Access changed: read again what the person may see. A second refusal
    /// in a row stops reconnecting until the followed scope or the
    /// capability list changes.
    async fn refused(&mut self, scope: &StreamScope) -> Next {
        self.refusals += 1;
        catch_up(self.weak, scope.generation).await;
        refresh_scope(self.weak, self.client);
        if self.refusals < MAX_REFUSALS {
            return Next::Reconnect;
        }
        if self.status(LiveStatus::Off, Some(scope.generation)) == Next::Stop {
            return Next::Stop;
        }
        let epoch = scope_epoch(self.weak);
        wait_for_scope_or_epoch_change(self.weak, scope, epoch).await;
        self.refusals = 0;
        Next::Again
    }

    /// Reconnects after the backoff, never sooner than a server's wait.
    async fn back_off(&mut self, scope: &StreamScope, error: LiveError) -> Next {
        if self.status(LiveStatus::Reconnecting, Some(scope.generation)) == Next::Stop {
            return Next::Stop;
        }
        let floor = self.floor(error);
        let delay = self.backoff.next(rand::random_range(0.0..=1.0)).max(floor);
        tokio::select! {
            () = wait_for_scope_change(self.weak, scope) => {
                self.backoff.reset();
                self.status(LiveStatus::Connecting, None)
            }
            () = tokio::time::sleep(delay) => Next::Again,
        }
    }

    /// A server's wait, honoured as every NETT wait is: never shortened, for
    /// at most five minutes, and kept for the rounds that follow.
    fn floor(&mut self, error: LiveError) -> Duration {
        let LiveError::Busy(Some(seconds)) = error else {
            return Duration::ZERO;
        };
        let floor = crate::nett::flow::pause(0, Some(seconds), 0.0);
        self.not_before = Some(crate::nett::flow::deadline(Instant::now(), floor));
        floor
    }
}

/// The server's wait is kept even when the followed teams change.
async fn hold(weak: &Weak<Shared>, generation: ScopeGeneration, wait: Duration) {
    tokio::select! {
        () = wait_for_generation_change(weak, generation) => {}
        () = tokio::time::sleep(wait) => {}
    }
}

/// Until the hub's scope generation changes or the hub stops.
async fn wait_for_generation_change(weak: &Weak<Shared>, generation: ScopeGeneration) {
    while super::scope(weak).is_some_and(|scope| scope.generation == generation) {
        tokio::time::sleep(SLICE).await;
    }
}

/// Reads the open issue and the list head again after a close, and waits,
/// up to a bound, until those reads are back.
async fn catch_up(weak: &Weak<Shared>, generation: ScopeGeneration) {
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

/// NETT accepts at most 64 team keys on one stream. Past that, the open
/// issue's team and the chosen team come first, then the rest in order.
pub(super) fn bounded(projects: BTreeSet<String>, state: &State) -> BTreeSet<String> {
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
