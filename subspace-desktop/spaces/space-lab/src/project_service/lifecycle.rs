use super::InputRevision;
use crate::project::{InputValue, LiveState, LoadedProject, ProjectRevision};
use crate::project_runtime::{Session, SessionStatus};
use anyhow::{Result, ensure};
use std::collections::VecDeque;
use std::sync::Arc;

/// A value-only readback. Private environment values are never present.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ProjectPresentation {
    pub working: LoadedProject,
    pub live: LiveState,
    pub status: SessionStatus,
    pub diagnostics: String,
    pub error: Option<String>,
    pub applied_revision: Option<ProjectRevision>,
}

impl ProjectPresentation {
    pub fn new(working: LoadedProject, live: LiveState, status: SessionStatus) -> Self {
        Self {
            working,
            live,
            status,
            diagnostics: String::new(),
            error: None,
            applied_revision: None,
        }
    }
}

pub(super) struct Slot {
    // A pending launch retains its token, so close/reopen cannot reuse its identity.
    pub(super) generation: Arc<()>,
    live: LiveState,
    pub session: Option<Session>,
    pub(super) applied_revision: Option<ProjectRevision>,
    pub(super) error: Option<String>,
    pub(super) outcomes: VecDeque<super::ActionOutcome>,
}

impl Slot {
    pub fn new(working: &LoadedProject) -> Result<Self> {
        Ok(Self {
            generation: Arc::new(()),
            live: LiveState::new(working.definition.clone(), "stopped")?,
            session: None,
            applied_revision: None,
            error: None,
            outcomes: VecDeque::new(),
        })
    }

    pub(super) fn live_state(&self) -> Result<LiveState> {
        self.session
            .as_ref()
            .map_or_else(|| Ok(self.live.clone()), Session::live_state)
    }

    pub fn presentation(&self, working: LoadedProject) -> Result<ProjectPresentation> {
        let mut presentation =
            ProjectPresentation::new(working, self.live.clone(), SessionStatus::Stopped);
        if let Some(session) = &self.session {
            let snapshot = session.snapshot()?;
            presentation.live = snapshot.live_state().clone();
            presentation.status = snapshot.status();
            presentation.diagnostics = snapshot.diagnostics().to_owned();
        }
        if self.error.is_some() {
            presentation.status = SessionStatus::Failed;
        }
        presentation.error = self.error.clone();
        presentation.applied_revision = self.applied_revision.clone();
        Ok(presentation)
    }

    pub fn stop(&mut self) -> Result<()> {
        self.generation = Arc::new(());
        if let Some(session) = &self.session {
            session.stop()?;
            self.live = session.live_state()?;
        }
        self.collect_outcomes()?;
        self.session = None;
        Ok(())
    }

    pub fn validate_revision(&self, expected: &InputRevision) -> Result<()> {
        let current = self.live_state()?;
        ensure!(
            current.session_id() == expected.session_id
                && current.input_revision() == expected.input_revision,
            "Project session or input revision conflict"
        );
        Ok(())
    }

    pub fn set_value(
        &mut self,
        key: &str,
        value: InputValue,
        expected: &InputRevision,
    ) -> Result<u64> {
        self.validate_revision(expected)?;
        match &self.session {
            Some(session) => session.set_value(key, value, expected.input_revision),
            None => self.live.set_value(key, value, expected.input_revision),
        }
    }
}
