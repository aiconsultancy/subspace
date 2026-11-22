//! Root-owned project state shared by native views and agent tools.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::project::ProjectStore;

mod files;
mod launch;
mod lifecycle;
mod outcomes;
mod registry;
mod session;
mod state;

pub use lifecycle::ProjectPresentation;
pub use outcomes::ActionOutcome;
pub use registry::{shared, shutdown_all};

/// Binds an input or action to the visible runtime, including after a restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct InputRevision {
    pub session_id: String,
    pub input_revision: u64,
}

impl InputRevision {
    pub fn new(session_id: impl Into<String>, input_revision: u64) -> Self {
        Self {
            session_id: session_id.into(),
            input_revision,
        }
    }
}

/// Filesystem and runtime operations block. Call them off the UI thread;
/// subscribing to changes only creates an in-memory watch receiver.
pub struct ProjectService {
    store: ProjectStore,
    state: Mutex<state::ServiceState>,
    // One shared dirty/version slot, not a queue of published patches. The
    // runtime keeps every accepted state change; each view chooses when to read.
    changes: tokio::sync::watch::Sender<()>,
}

impl ProjectService {
    /// Subscribe to coalesced changes; read the latest presentation after a wake.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<()> {
        self.changes.subscribe()
    }
}

#[cfg(test)]
mod tests;
