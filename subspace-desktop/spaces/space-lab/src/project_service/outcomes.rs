use super::{ProjectService, lifecycle::Slot};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::VecDeque;

const OUTCOME_HISTORY: usize = 64;

/// Bounded shared history, so one observer cannot silently consume another's outcome.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ActionOutcome {
    pub session_id: String,
    pub request_id: String,
    pub error: Option<String>,
}

impl ActionOutcome {
    pub fn new(session_id: String, request_id: String, error: Option<String>) -> Self {
        Self {
            session_id,
            request_id,
            error,
        }
    }
}

impl ProjectService {
    pub fn action_outcomes(&self, id: &str) -> Result<Vec<ActionOutcome>> {
        let mut state = self.lock_state()?;
        let slot = state.slots.get_mut(id).context("Open the project first")?;
        slot.collect_outcomes()?;
        Ok(slot.outcomes.iter().cloned().collect())
    }
}

impl Slot {
    pub(super) fn collect_outcomes(&mut self) -> Result<()> {
        let Some(session) = &self.session else {
            return Ok(());
        };
        let session_id = session.live_state()?.session_id().to_owned();
        let completions = session.take_completions()?;
        for completion in completions {
            retain_outcome(
                &mut self.outcomes,
                ActionOutcome::new(
                    session_id.clone(),
                    completion.request_id().to_owned(),
                    completion.error().map(str::to_owned),
                ),
            );
        }
        Ok(())
    }
}

fn retain_outcome(history: &mut VecDeque<ActionOutcome>, outcome: ActionOutcome) {
    history.push_back(outcome);
    if history.len() > OUTCOME_HISTORY {
        history.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionOutcome, OUTCOME_HISTORY, retain_outcome};
    use std::collections::VecDeque;

    #[test]
    fn history_retains_distinct_outcomes_and_evicts_only_the_oldest_at_capacity() {
        let mut history = VecDeque::new();
        for request in 0..=OUTCOME_HISTORY {
            retain_outcome(
                &mut history,
                ActionOutcome::new("fixture-session".into(), format!("request-{request}"), None),
            );
        }
        assert_eq!(history.len(), OUTCOME_HISTORY);
        assert_eq!(
            history.front().expect("oldest retained").request_id,
            "request-1"
        );
        assert_eq!(history.back().expect("newest").request_id, "request-64");
    }
}
