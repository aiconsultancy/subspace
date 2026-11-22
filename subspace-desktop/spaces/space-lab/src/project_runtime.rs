//! Owned persistent project execution, independent of the selected board view.
//!
//! Preparation must finish before the owner stops an existing session. All
//! lifecycle methods perform blocking work and belong off the UI thread.

mod diagnostics;
mod framing;
mod preparation;
mod proxy;
mod state;
mod supervisor;

#[cfg(test)]
mod tests;

use crate::environments::EnvironmentManager;
use crate::project::{InputValue, LiveState, SourceSnapshot};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::environments::persistent::PreparedRuntime;
use proxy::DestinationPolicy;
use state::RuntimeState;
use supervisor::{Owner, lock};

const MAX_PENDING: usize = 64;

/// Validated source and immutable environment staged without executing code.
/// Selected values are supplied explicitly; this boundary never reads settings.
pub struct PreparedSession {
    snapshot: SourceSnapshot,
    runtime: PreparedRuntime,
    policy: DestinationPolicy,
    values: BTreeMap<String, InputValue>,
}

impl PreparedSession {
    pub fn new(
        manager: &EnvironmentManager,
        snapshot: SourceSnapshot,
        selected_environment: BTreeMap<String, String>,
    ) -> Result<Self> {
        preparation::prepare(manager, snapshot, selected_environment)
    }

    /// Merge known inputs into defaults and validate the complete accepted map
    /// without execution. Unknown keys or incompatible values reject preparation.
    pub fn with_values(mut self, values: BTreeMap<String, InputValue>) -> Result<Self> {
        self.values = preparation::validated_values(self.snapshot.definition(), values)?;
        Ok(self)
    }

    pub fn snapshot(&self) -> &SourceSnapshot {
        &self.snapshot
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Starting,
    Running,
    Stopped,
    Failed,
}

/// An action outcome is retained until explicitly taken, never coalesced with
/// replaceable display traffic. An error does not freshen retained results.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Completion {
    request_id: String,
    error: Option<String>,
}

impl Completion {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// A coherent presentation copy of the latest accepted state.
#[derive(Clone, Debug)]
pub struct SessionSnapshot {
    live_state: LiveState,
    status: SessionStatus,
    diagnostics: String,
    pending: usize,
}

impl SessionSnapshot {
    // qual:allow(dry, boilerplate) reason: "Readonly session presentation preserves one coherent accepted state, status and diagnostic snapshot."
    pub fn live_state(&self) -> &LiveState {
        &self.live_state
    }
    // qual:allow(dry, boilerplate) reason: "The status is readonly evidence belonging to the same accepted session snapshot."
    pub fn status(&self) -> SessionStatus {
        self.status
    }
    // qual:allow(dry, boilerplate) reason: "Readonly diagnostics cannot be replaced independently of the session snapshot that owns their redaction boundary."
    pub fn diagnostics(&self) -> &str {
        &self.diagnostics
    }
    // qual:allow(dry, boilerplate) reason: "Pending work is readonly supervisor evidence, not a caller-editable queue size."
    pub fn pending(&self) -> usize {
        self.pending
    }
}

/// Shared ownership handle for UI and tools. The final handle's drop stops and
/// reaps owned execution. Closing a view alone must not drop the root's handle.
#[derive(Clone)]
pub struct Session {
    owner: Arc<Owner>,
    delivery: Arc<Mutex<()>>,
}

impl Session {
    pub fn start(prepared: PreparedSession) -> Result<Self> {
        Self::start_with_changes(prepared, tokio::sync::watch::channel(()).0)
    }

    pub(crate) fn start_with_changes(
        prepared: PreparedSession,
        changes: tokio::sync::watch::Sender<()>,
    ) -> Result<Self> {
        let session_id = format!(
            "session-{}-{}",
            std::process::id(),
            crate::environments::timestamp_nanos()
        );
        let mut state = RuntimeState::new(
            prepared.snapshot.definition().clone(),
            &session_id,
            Some(changes),
        )?;
        state.configure(
            prepared.snapshot.revision().clone(),
            prepared
                .runtime
                .request
                .environment
                .iter()
                .map(|(_, value)| value.clone())
                .collect(),
        );
        state.seed_values(prepared.values)?;
        let proxy = proxy::PersistentProxy::start(prepared.policy)?;
        let owner = Owner::start(prepared.runtime, state, proxy)?;
        Ok(Self {
            owner: Arc::new(owner),
            delivery: Arc::new(Mutex::new(())),
        })
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot> {
        self.owner.snapshot()
    }

    pub fn status(&self) -> Result<SessionStatus> {
        Ok(lock(&self.owner.state)?.status())
    }

    pub fn live_state(&self) -> Result<LiveState> {
        Ok(lock(&self.owner.state)?.live().clone())
    }

    pub fn set_value(&self, key: &str, value: InputValue, expected: u64) -> Result<u64> {
        let _delivery = lock(&self.delivery)?;
        let message = lock(&self.owner.state)?.set_value(key, value, expected)?;
        let revision = message.context.input_revision;
        self.owner.queue(message)?;
        Ok(revision)
    }

    pub fn invoke(&self, action: &str, expected: u64) -> Result<String> {
        let _delivery = lock(&self.delivery)?;
        let message = lock(&self.owner.state)?.invoke(action, expected)?;
        let request_id = message.context.request_id.clone();
        self.owner.queue(message)?;
        Ok(request_id)
    }

    pub fn take_completions(&self) -> Result<Vec<Completion>> {
        Ok(lock(&self.owner.state)?.take_completions())
    }

    pub fn stop(&self) -> Result<()> {
        self.owner.stop()
    }
}
