use anyhow::{Context, Result, ensure};

use super::{InputRevision, ProjectPresentation, ProjectService, lifecycle::Slot};
use crate::project::{InputValue, LiveState, ProjectRevision};

impl ProjectService {
    pub fn open(&self, id: &str) -> Result<ProjectPresentation> {
        let working = self.load(id)?;
        let mut state = self.lock_state()?;
        ensure!(!state.shutdown, "Project service has shut down");
        let slot = match state.slots.entry(id.to_owned()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => entry.insert(Slot::new(&working)?),
        };
        slot.presentation(working)
    }

    pub fn live_state(&self, id: &str) -> Result<LiveState> {
        let state = self.lock_state()?;
        state
            .slots
            .get(id)
            .context("Open the project first")?
            .live_state()
    }

    pub fn start(&self, id: &str, expected: &ProjectRevision) -> Result<ProjectPresentation> {
        self.launch(id, expected, false)
    }

    pub fn apply_and_restart(
        &self,
        id: &str,
        expected: &ProjectRevision,
    ) -> Result<ProjectPresentation> {
        self.launch(id, expected, true)
    }

    pub fn stop(&self, id: &str) -> Result<()> {
        let mut state = self.lock_state()?;
        if let Some(slot) = state.slots.get_mut(id) {
            slot.stop()?;
            self.changes.send_replace(());
        }
        Ok(())
    }

    pub fn close(&self, id: &str) -> Result<()> {
        let mut state = self.lock_state()?;
        if let Some(slot) = state.slots.get_mut(id) {
            slot.stop()?;
        }
        state.slots.remove(id);
        self.changes.send_replace(());
        Ok(())
    }

    pub fn set_value(
        &self,
        id: &str,
        key: &str,
        value: InputValue,
        expected: &InputRevision,
    ) -> Result<u64> {
        let mut state = self.lock_state()?;
        let revision = state
            .slots
            .get_mut(id)
            .context("Open the project first")?
            .set_value(key, value, expected)?;
        self.changes.send_replace(());
        Ok(revision)
    }

    pub fn invoke(&self, id: &str, action: &str, expected: &InputRevision) -> Result<String> {
        let state = self.lock_state()?;
        let slot = state.slots.get(id).context("Open the project first")?;
        slot.validate_revision(expected)?;
        let session = slot.session.as_ref().context("Start the project first")?;
        session.invoke(action, expected.input_revision)
    }

    pub fn shutdown(&self) -> Result<()> {
        let mut state = self.lock_state()?;
        state.shutdown = true;
        let mut errors = Vec::new();
        for slot in state.slots.values_mut() {
            if let Err(error) = slot.stop() {
                errors.push(error.to_string());
            }
        }
        ensure!(
            errors.is_empty(),
            "Some project workers could not stop: {}",
            errors.join("; ")
        );
        state.slots.clear();
        self.changes.send_replace(());
        Ok(())
    }
}
