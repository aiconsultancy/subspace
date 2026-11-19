use anyhow::{Context, Result, ensure};

use super::{ProjectPresentation, ProjectService, lifecycle::Slot};
use crate::environments::EnvironmentManager;
use crate::private_settings::PrivateSettings;
use crate::project::{LiveState, LoadedProject, ProjectRevision};
use crate::project_runtime::{PreparedSession, Session};

mod identity;

impl ProjectService {
    pub(super) fn launch(
        &self,
        id: &str,
        expected: &ProjectRevision,
        replace: bool,
    ) -> Result<ProjectPresentation> {
        self.launch_with(id, expected, replace, |working, live| {
            self.prepare(id, expected, working, live)
        })
    }

    pub(super) fn launch_with(
        &self,
        id: &str,
        expected: &ProjectRevision,
        replace: bool,
        prepare: impl FnOnce(&LoadedProject, &LiveState) -> Result<PreparedSession>,
    ) -> Result<ProjectPresentation> {
        let working = self.load(id)?;
        ensure!(&working.revision == expected, "Project revision conflict");
        let identity = self.capture_launch(&working, replace)?;
        // Staging may wait for DNS, filesystem or private-store locks. It must
        // neither execute project code nor prevent lifecycle cancellation.
        let prepared = prepare(&working, &identity.live);
        let current = self.load(id);
        let mut state = self.lock_state()?;
        identity.validate(&state, id)?;
        ensure!(
            current?.revision == working.revision,
            "Project launch conflict: source changed"
        );
        let slot = state.slots.get_mut(id).context("Project launch conflict")?;
        let result = slot.replace_session(working, prepared?, self.changes.clone());
        self.changes.send_replace(());
        result
    }

    fn prepare(
        &self,
        id: &str,
        expected: &ProjectRevision,
        working: &LoadedProject,
        live: &LiveState,
    ) -> Result<PreparedSession> {
        let snapshot = self.store.snapshot(id, expected)?;
        let settings = PrivateSettings::new(self.root(), id)?;
        let names = working
            .definition
            .environment_keys
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let environment = settings.selected(&names)?.into_values();
        let manager = EnvironmentManager::new(self.root().join("environments"));
        let values = live
            .values()
            .iter()
            .filter(|(key, _)| working.definition.inputs.contains_key(*key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        PreparedSession::new(&manager, snapshot, environment)?.with_values(values)
    }
}

impl Slot {
    fn replace_session(
        &mut self,
        working: LoadedProject,
        prepared: PreparedSession,
        changes: tokio::sync::watch::Sender<()>,
    ) -> Result<ProjectPresentation> {
        // All replacement validation and staging precede this destructive boundary.
        self.stop()?;
        match Session::start_with_changes(prepared, changes) {
            Ok(session) => {
                self.session = Some(session);
                self.error = None;
                self.applied_revision = Some(working.revision.clone());
                self.presentation(working)
            }
            Err(error) => {
                self.error = Some(error.to_string());
                Err(error)
            }
        }
    }
}
