//! Pure cloud-agent task-state rules: which states keep a task alive, which
//! end it, which transitions deserve an alert, and how often a watcher polls.
//! No requests and no GPUI, so dashboards and background watchers share them.

use std::time::Duration;

/// The documented states a task or session can report.
/// `idle` counts as finished: the agent stopped without more work to do.
pub const ACTIVE_STATES: &[&str] = &["queued", "in_progress", "waiting_for_user"];
pub const FINISHED_STATES: &[&str] = &["completed", "failed", "timed_out", "cancelled", "idle"];

/// How often a watcher asks GitHub about its active tasks.
pub const POLL_INTERVAL: Duration = Duration::from_secs(30);
/// The slowest poll a watcher backs off to after errors or rate limits.
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(120);

/// True while the task can still change on its own. `waiting_for_user` stays
/// active because the user can answer and the task continues.
pub fn is_active(state: &str) -> bool {
    ACTIVE_STATES.contains(&state)
}

/// True for the documented end states. An unknown future state is neither
/// active nor finished, so a watcher keeps showing it without alerting.
// qual:api
pub fn is_finished(state: &str) -> bool {
    FINISHED_STATES.contains(&state)
}

// qual:api
pub fn needs_input(state: &str) -> bool {
    state == "waiting_for_user"
}

/// What a monitored task's change is worth telling the user about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    NeedsInput,
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    /// The agent stopped with nothing left to do.
    Idle,
}

/// Every state worth an alert, with the alert it raises and the phrase a
/// toast or a notification reads.
const ALERTS: &[(&str, AlertKind, &str)] = &[
    ("waiting_for_user", AlertKind::NeedsInput, "needs input"),
    ("completed", AlertKind::Completed, "completed"),
    ("failed", AlertKind::Failed, "failed"),
    ("timed_out", AlertKind::TimedOut, "timed out"),
    ("cancelled", AlertKind::Cancelled, "was cancelled"),
    ("idle", AlertKind::Idle, "went idle"),
];

impl AlertKind {
    /// A short phrase for a toast or a notification body.
    pub fn label(self) -> &'static str {
        ALERTS
            .iter()
            .find(|(_, kind, _)| *kind == self)
            .map_or("changed", |(_, _, label)| *label)
    }
}

/// The alert a state change earns, or `None` when it earns none. Only a task
/// that was active can alert, so a list refresh of old finished tasks is
/// silent, and an unknown state never alerts.
// qual:api
pub fn alert_for(previous: &str, current: &str) -> Option<AlertKind> {
    if previous == current || !is_active(previous) {
        return None;
    }
    ALERTS
        .iter()
        .find(|(state, _, _)| *state == current)
        .map(|(_, kind, _)| *kind)
}

/// The watcher's poll delay: [`POLL_INTERVAL`] while requests succeed,
/// doubling up to [`MAX_POLL_INTERVAL`] after an error or a rate limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollBackoff {
    interval: Duration,
}

impl Default for PollBackoff {
    fn default() -> Self {
        Self {
            interval: POLL_INTERVAL,
        }
    }
}

impl PollBackoff {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Slows the next poll after a failed request.
    pub fn failed(&mut self) -> Duration {
        self.interval = (self.interval * 2).min(MAX_POLL_INTERVAL);
        self.interval
    }

    /// Returns to the normal interval after a successful request.
    pub fn succeeded(&mut self) -> Duration {
        self.interval = POLL_INTERVAL;
        self.interval
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ACTIVE_STATES, AlertKind, FINISHED_STATES, MAX_POLL_INTERVAL, POLL_INTERVAL, PollBackoff,
        alert_for, is_active, is_finished, needs_input,
    };

    #[test]
    fn active_and_finished_states_are_disjoint_and_cover_the_documented_set() {
        for state in ACTIVE_STATES {
            assert!(is_active(state), "{state} is active");
            assert!(!is_finished(state), "{state} is not finished");
        }
        for state in FINISHED_STATES {
            assert!(is_finished(state), "{state} is finished");
            assert!(!is_active(state), "{state} is not active");
        }
        assert!(!is_active("new_state"));
        assert!(!is_finished("new_state"));
        assert!(needs_input("waiting_for_user"));
        assert!(!needs_input("in_progress"));
    }

    #[test]
    fn alerts_fire_only_when_an_active_task_reaches_a_reportable_state() {
        assert_eq!(
            alert_for("in_progress", "waiting_for_user"),
            Some(AlertKind::NeedsInput)
        );
        assert_eq!(
            alert_for("waiting_for_user", "completed"),
            Some(AlertKind::Completed)
        );
        assert_eq!(alert_for("queued", "failed"), Some(AlertKind::Failed));
        assert_eq!(
            alert_for("in_progress", "timed_out"),
            Some(AlertKind::TimedOut)
        );
        assert_eq!(
            alert_for("in_progress", "cancelled"),
            Some(AlertKind::Cancelled)
        );
        assert_eq!(alert_for("in_progress", "idle"), Some(AlertKind::Idle));
        assert_eq!(alert_for("in_progress", "in_progress"), None);
        assert_eq!(alert_for("queued", "in_progress"), None);
        assert_eq!(alert_for("completed", "failed"), None);
        assert_eq!(alert_for("in_progress", "new_state"), None);
        assert_eq!(AlertKind::NeedsInput.label(), "needs input");
        assert_eq!(AlertKind::TimedOut.label(), "timed out");
    }

    #[test]
    fn backoff_doubles_to_the_cap_and_a_success_resets_it() {
        let mut backoff = PollBackoff::new();
        assert_eq!(backoff.interval(), POLL_INTERVAL);
        assert_eq!(backoff.failed(), POLL_INTERVAL * 2);
        assert_eq!(backoff.failed(), MAX_POLL_INTERVAL);
        assert_eq!(backoff.failed(), MAX_POLL_INTERVAL);
        assert_eq!(backoff.succeeded(), POLL_INTERVAL);
        assert_eq!(backoff.interval(), POLL_INTERVAL);
    }
}
