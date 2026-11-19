//! Where one tab's conversation is: waiting for its runtime, idle, running a
//! turn, or cut off from a runtime that stopped. Only a turn's end or a lost
//! runtime ends a running turn; an error the runtime reports during it is a
//! notice, and the turn goes on.

use std::time::Instant;

use subspace_agent::TurnFailure;

use super::status_words;
use super::tabs::{LastTurn, Tab};

#[derive(Debug, Default)]
pub(super) enum TabRun {
    /// The runtime has not reported this chat ready yet.
    #[default]
    Starting,
    Idle,
    Running(RunningTurn),
    /// The runtime stopped or stopped answering. Only a new runtime, or the
    /// silent one recovering, has the chat ready again.
    Disconnected,
}

/// The turn a tab runs, for its status line and its alerts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RunningTurn {
    pub(super) started: Instant,
    /// Seeds the turn's status phrases, so turns in a row open differently.
    pub(super) seed: u64,
    /// The status phrase shown.
    pub(super) phrase: usize,
    /// The turn started from this live panel, so its requests and its end
    /// may alert. Stop withdraws it.
    pub(super) notify: bool,
}

impl RunningTurn {
    fn new(seed: u64, notify: bool) -> Self {
        Self {
            started: Instant::now(),
            seed,
            phrase: status_words::index(seed, 0),
            notify,
        }
    }

    /// Moves the status phrase on, and says whether it changed.
    pub(super) fn advance(&mut self) -> bool {
        let phrase = status_words::index(self.seed, status_words::ticks(self.started.elapsed()));
        let changed = phrase != self.phrase;
        self.phrase = phrase;
        changed
    }
}

/// Why the last turn failed, until the next turn starts, and whether its
/// Details disclosure is open.
#[derive(Clone, Debug)]
pub(super) struct ShownFailure {
    pub(super) failure: TurnFailure,
    pub(super) details_open: bool,
}

impl ShownFailure {
    pub(super) fn new(failure: TurnFailure) -> Self {
        Self {
            failure,
            details_open: false,
        }
    }
}

impl Tab {
    /// The runtime has this chat and takes its messages.
    pub(super) fn is_ready(&self) -> bool {
        matches!(self.run, TabRun::Idle | TabRun::Running(_))
    }

    /// A turn is running in this tab.
    pub(super) fn is_busy(&self) -> bool {
        matches!(self.run, TabRun::Running(_))
    }

    pub(super) fn running(&self) -> Option<&RunningTurn> {
        match &self.run {
            TabRun::Running(turn) => Some(turn),
            _ => None,
        }
    }

    pub(super) fn running_mut(&mut self) -> Option<&mut RunningTurn> {
        match &mut self.run {
            TabRun::Running(turn) => Some(turn),
            _ => None,
        }
    }

    /// The runtime reported the chat ready. A running turn goes on.
    pub(super) fn ready(&mut self) {
        if !self.is_busy() {
            self.run = TabRun::Idle;
        }
    }

    /// A sent message starts a turn, with a new seed so the status line
    /// opens on a different phrase. A message sent while a turn runs steers
    /// it, keeping its seed, timer and alerts. Either way the last error and
    /// failure are cleared.
    pub(super) fn begin(&mut self) {
        self.error = None;
        self.errors.legacy = None;
        self.failure = None;
        self.context.clear();
        if !self.is_busy() {
            self.run = TabRun::Running(self.next_turn(true));
        }
    }

    /// The runtime accepted `turn` as a new turn.
    pub(super) fn sent(&mut self, turn: LastTurn) {
        self.last_turn = Some(turn);
        self.begin();
    }

    /// Output arrived with no turn running here, as for a turn this panel did
    /// not start. It runs, timed, without alerts.
    pub(super) fn resume(&mut self) {
        if matches!(self.run, TabRun::Idle) {
            self.run = TabRun::Running(self.next_turn(false));
        }
    }

    /// The turn ended, and its open requests with it. Returns whether its
    /// end alerts: a turn that ends with an error or a failure showing does
    /// not announce that it completed.
    pub(super) fn end(&mut self) -> bool {
        self.pending.clear();
        let notify = self.running().is_some_and(|turn| turn.notify);
        if self.is_busy() {
            self.run = TabRun::Idle;
        }
        notify && self.error.is_none() && self.failure.is_none()
    }

    /// The runtime stopped or stopped answering: a running turn ends with
    /// its requests, and the chat is not ready. Returns whether a turn ended.
    pub(super) fn disconnect(&mut self) -> bool {
        self.pending.clear();
        let ended = self.is_busy();
        self.run = TabRun::Disconnected;
        ended
    }

    /// The running turn no longer alerts, as after Stop.
    pub(super) fn cancel_completion_alert(&mut self) {
        if let Some(turn) = self.running_mut() {
            turn.notify = false;
        }
    }

    fn next_turn(&mut self, notify: bool) -> RunningTurn {
        self.turns = self.turns.wrapping_add(1);
        RunningTurn::new(self.turns, notify)
    }
}

#[cfg(test)]
mod tests {
    use subspace_agent::failure::TurnContext;
    use subspace_agent::{FailureCategory, TurnFailure};

    use super::super::Pending;
    use super::super::tabs::Tab;
    use super::{ShownFailure, TabRun};

    fn approval() -> Pending {
        Pending::Approval {
            id: "write-1".into(),
            summary: "Update task".into(),
        }
    }

    #[test]
    fn a_new_tab_starts_and_becomes_idle_when_ready() {
        let mut tab = Tab::new(1);
        assert!(matches!(tab.run, TabRun::Starting));
        assert!(!tab.is_ready() && !tab.is_busy());
        tab.ready();
        assert!(matches!(tab.run, TabRun::Idle));
        assert!(tab.is_ready());
    }

    #[test]
    fn ready_leaves_a_running_turn_running() {
        let mut tab = Tab::new(1);
        tab.begin();
        let turn = tab.running().copied();
        tab.ready();
        assert_eq!(tab.running().copied(), turn);
    }

    #[test]
    fn every_end_clears_the_open_requests() {
        let mut idle = Tab::new(1);
        idle.ready();
        idle.pending.push(approval());
        assert!(!idle.end(), "nothing ran, so nothing alerts");
        assert!(idle.pending.is_empty());

        let mut running = Tab::new(1);
        running.begin();
        running.pending.push(approval());
        assert!(running.end());
        assert!(running.pending.is_empty());
        assert!(matches!(running.run, TabRun::Idle));

        let mut lost = Tab::new(1);
        lost.begin();
        lost.pending.push(approval());
        assert!(lost.disconnect(), "a running turn ended");
        assert!(lost.pending.is_empty());
        assert!(matches!(lost.run, TabRun::Disconnected));
    }

    #[test]
    fn a_turn_that_ends_with_an_error_or_failure_showing_does_not_alert() {
        let mut errored = Tab::new(1);
        errored.begin();
        errored.error = Some("canvas failed".into());
        assert!(!errored.end());

        let mut failed = Tab::new(1);
        failed.begin();
        failed.failure = Some(ShownFailure::new(TurnFailure::stopped(
            FailureCategory::NoResponse,
            "fixture",
            &TurnContext::default(),
            None,
        )));
        assert!(!failed.end());

        let mut stopped = Tab::new(1);
        stopped.begin();
        stopped.cancel_completion_alert();
        assert!(!stopped.end());
    }

    #[test]
    fn output_while_idle_runs_a_timed_turn_without_alerts() {
        let mut tab = Tab::new(1);
        tab.ready();
        tab.resume();
        let turn = tab.running().copied().expect("a running turn");
        assert!(!turn.notify);
        assert_eq!(turn.seed, tab.turns);
        tab.resume();
        assert_eq!(tab.running().copied(), Some(turn), "it keeps its timer");

        let mut starting = Tab::new(1);
        starting.resume();
        assert!(matches!(starting.run, TabRun::Starting));
        let mut lost = Tab::new(1);
        lost.disconnect();
        lost.resume();
        assert!(matches!(lost.run, TabRun::Disconnected));
    }
}
