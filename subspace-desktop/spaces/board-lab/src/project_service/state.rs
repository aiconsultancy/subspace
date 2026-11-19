use std::collections::BTreeMap;
use std::sync::{Arc, MutexGuard};

use anyhow::{Result, ensure};

use super::{ProjectService, lifecycle::Slot};

#[derive(Default)]
pub(super) struct ServiceState {
    pub slots: BTreeMap<String, Slot>,
    pub sources: BTreeMap<String, SourceIntent>,
    pub shutdown: bool,
}

#[derive(Default)]
pub(super) struct SourceIntent {
    pub identity: Arc<()>,
    pub writers: usize,
}

/// Tracks a source write without holding the state lock across filesystem work.
struct SourceChange<'a> {
    service: &'a ProjectService,
    id: String,
}

impl ProjectService {
    pub(super) fn lock_state(&self) -> Result<MutexGuard<'_, ServiceState>> {
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("Project state unavailable"))
    }

    pub(super) fn change_source<T>(
        &self,
        id: &str,
        change: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let _change = self.begin_source_change(id)?;
        change()
    }

    fn begin_source_change(&self, id: &str) -> Result<SourceChange<'_>> {
        let mut state = self.lock_state()?;
        ensure!(!state.shutdown, "Project service has shut down");
        let intent = state.sources.entry(id.to_owned()).or_default();
        intent.writers = intent
            .writers
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Too many project source edits"))?;
        intent.identity = Arc::new(());
        Ok(SourceChange {
            service: self,
            id: id.to_owned(),
        })
    }
}

impl Drop for SourceChange<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.service.lock_state()
            && let Some(intent) = state.sources.get_mut(&self.id)
        {
            intent.writers -= 1;
            intent.identity = Arc::new(());
        }
    }
}
