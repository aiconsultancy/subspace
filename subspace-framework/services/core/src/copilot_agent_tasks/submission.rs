//! One cloud-task attempt: a shared deadline and atomic dispatch admission.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const SUBMISSION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct Submission {
    deadline: Instant,
    stage: Arc<Mutex<Stage>>,
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Preflight,
    Dispatched,
    Expired(SubmissionTimeout),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionTimeout {
    #[error("The submission expired before task creation dispatch")]
    NotSent,
    #[error("The submission timed out after task creation dispatch began")]
    Unknown,
}

impl Default for Submission {
    fn default() -> Self {
        Self::new(default_deadline())
    }
}

fn default_deadline() -> Instant {
    Instant::now() + SUBMISSION_TIMEOUT
}

impl Submission {
    pub fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            stage: Arc::new(Mutex::new(Stage::Preflight)),
        }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn remaining(&self) -> Result<Duration, SubmissionTimeout> {
        budget(
            self.deadline,
            &mut self.stage.lock().unwrap_or_else(|p| p.into_inner()),
        )
    }

    /// Serialize the last dispatch gate with expiration. Once admitted, a
    /// request may have reached GitHub; expiration must then report unknown.
    /// Explicit API-version rejection may use the gate again, never a blind retry.
    pub fn begin_dispatch(&self) -> Result<Duration, SubmissionTimeout> {
        let mut stage = self.stage.lock().unwrap_or_else(|p| p.into_inner());
        let remaining = budget(self.deadline, &mut stage)?;
        *stage = Stage::Dispatched;
        Ok(remaining)
    }

    pub fn expire(&self) -> SubmissionTimeout {
        expire(&mut self.stage.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

fn budget(deadline: Instant, stage: &mut Stage) -> Result<Duration, SubmissionTimeout> {
    if let Stage::Expired(outcome) = stage {
        return Err(*outcome);
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| expire(stage))
}

fn expire(stage: &mut Stage) -> SubmissionTimeout {
    let outcome = match *stage {
        Stage::Preflight => SubmissionTimeout::NotSent,
        Stage::Dispatched => SubmissionTimeout::Unknown,
        Stage::Expired(outcome) => outcome,
    };
    *stage = Stage::Expired(outcome);
    outcome
}

#[cfg(test)]
mod tests {
    use super::{Submission, SubmissionTimeout};
    use std::time::{Duration, Instant};

    #[test]
    fn expiry_blocks_late_dispatch_and_preserves_an_already_admitted_outcome() {
        let before = Submission::default();
        let copy = before.clone();
        assert_eq!(before.expire(), SubmissionTimeout::NotSent);
        assert_eq!(copy.begin_dispatch(), Err(SubmissionTimeout::NotSent));
        let after = Submission::default();
        assert!(after.begin_dispatch().is_ok());
        assert_eq!(after.expire(), SubmissionTimeout::Unknown);
        assert_eq!(after.begin_dispatch(), Err(SubmissionTimeout::Unknown));
    }

    #[test]
    fn the_absolute_deadline_also_blocks_dispatch_without_a_ui_timer() {
        let submission = Submission::new(Instant::now() - Duration::from_secs(1));
        assert_eq!(submission.begin_dispatch(), Err(SubmissionTimeout::NotSent));
        assert!(submission.remaining().is_err());
        let fresh = Submission::default();
        assert!(fresh.deadline() > Instant::now());
        assert!(fresh.remaining().is_ok());
    }
}
