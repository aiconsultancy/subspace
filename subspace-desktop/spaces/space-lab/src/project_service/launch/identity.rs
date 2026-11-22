use std::sync::Arc;

use anyhow::{Context, Result, ensure};

use crate::project::{LiveState, LoadedProject};
use crate::project_runtime::{Session, SessionStatus};
use crate::project_service::{InputRevision, ProjectService, lifecycle::Slot, state::ServiceState};

pub(super) struct LaunchIdentity {
    pub live: LiveState,
    generation: Arc<()>,
    source: Arc<()>,
}

impl ProjectService {
    pub(super) fn capture_launch(
        &self,
        working: &LoadedProject,
        replace: bool,
    ) -> Result<LaunchIdentity> {
        let id = &working.definition.id;
        let mut state = self.lock_state()?;
        ensure!(!state.shutdown, "Project service has shut down");
        let source = state.sources.entry(id.clone()).or_default();
        ensure!(
            source.writers == 0,
            "Project launch conflict: source edit pending"
        );
        let source = source.identity.clone();
        if !state.slots.contains_key(id) {
            state.slots.insert(id.clone(), Slot::new(working)?);
        }
        let slot = state
            .slots
            .get_mut(id)
            .context("Project state unavailable")?;
        slot.validate_start(replace)?;
        let live = slot.live_state()?;
        // Each request supersedes older staging. The old request retains its
        // Arc, so this generation cannot suffer pointer reuse or integer wrap.
        slot.generation = Arc::new(());
        Ok(LaunchIdentity {
            live,
            generation: slot.generation.clone(),
            source,
        })
    }
}

impl LaunchIdentity {
    pub(super) fn validate(&self, state: &ServiceState, id: &str) -> Result<()> {
        ensure!(
            !state.shutdown,
            "Project launch conflict: service shut down"
        );
        let slot = state
            .slots
            .get(id)
            .context("Project launch conflict: project closed")?;
        let source = state
            .sources
            .get(id)
            .context("Project launch conflict: source changed")?;
        ensure!(
            Arc::ptr_eq(&self.generation, &slot.generation)
                && Arc::ptr_eq(&self.source, &source.identity)
                && source.writers == 0,
            "Project launch conflict: cancelled or superseded"
        );
        slot.validate_revision(&InputRevision::new(
            self.live.session_id(),
            self.live.input_revision(),
        ))
    }
}

impl Slot {
    fn validate_start(&self, replace: bool) -> Result<()> {
        let status = self.session.as_ref().map(Session::status).transpose()?;
        validate_start_status(status, replace)
    }
}

fn validate_start_status(status: Option<SessionStatus>, replace: bool) -> Result<()> {
    ensure!(
        replace
            || status.is_none_or(|status| matches!(
                status,
                SessionStatus::Stopped | SessionStatus::Failed
            )),
        "Project is already running; use Apply & Restart"
    );
    Ok(())
}
