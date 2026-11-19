//! Token leases that never mix the outgoing and incoming account.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use subspace_account::{Account, Error, Provider, ServiceToken};

/// Closed from the start of a sign-in or sign-out until the shell records
/// its outcome, so no token lease can mix the outgoing and incoming account.
#[derive(Default)]
pub(crate) struct LeaseGate(AtomicBool);

impl LeaseGate {
    pub(super) fn close(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub(super) fn open(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    pub(super) fn is_open(&self) -> bool {
        !self.0.load(Ordering::SeqCst)
    }
}

/// A native product adapter's snapshot of the current account.
pub struct AccountSession {
    pub(super) manager: Arc<dyn Provider>,
    pub(super) gate: Arc<LeaseGate>,
    pub(super) account: Account,
    pub(super) generation: u64,
}

/// Gives a trusted desktop adapter silent access to a registered resource.
/// A lease taken while the account changes is refused, and the generation
/// advances when a new account is committed, so a client bound to an
/// earlier generation can tell that it is stale.
#[derive(Clone)]
pub struct AccountTokenSource {
    manager: Arc<dyn Provider>,
    gate: Arc<LeaseGate>,
}

impl AccountTokenSource {
    pub(crate) fn new(manager: Arc<dyn Provider>, gate: Arc<LeaseGate>) -> Self {
        Self { manager, gate }
    }

    pub fn generation(&self) -> u64 {
        self.manager.generation()
    }

    /// A token for the current account, or [`Error::Cancelled`] while the
    /// account is changing or if it changed during the request.
    // qual:api — consumed by trusted product adapters in the Desktop workspace.
    pub fn token_for(&self, resource: &str, minimum: Duration) -> Result<ServiceToken, Error> {
        lease(
            &self.manager,
            &self.gate,
            self.generation(),
            resource,
            minimum,
        )
    }
}

impl AccountSession {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    // qual:api — downstream adapters detect an account change before applying work.
    pub fn is_current(&self) -> bool {
        self.gate.is_open() && self.manager.generation() == self.generation
    }

    /// Obtain a resource token, or return an error if this account changed.
    /// Call off the UI thread and keep the credential in the native adapter.
    // qual:api — trusted native adapters require resource-scoped tokens.
    pub fn token_for(&self, resource: &str, minimum: Duration) -> Result<ServiceToken, Error> {
        let grant = lease(
            &self.manager,
            &self.gate,
            self.generation,
            resource,
            minimum,
        )?;
        if grant.account.tenant != self.account.tenant || grant.account.id != self.account.id {
            return Err(Error::Cancelled);
        }
        Ok(grant)
    }
}

/// Lease a token under `generation`, refusing it if the gate closed or the
/// generation moved on at any point before the token is handed out.
fn lease(
    manager: &Arc<dyn Provider>,
    gate: &LeaseGate,
    generation: u64,
    resource: &str,
    minimum: Duration,
) -> Result<ServiceToken, Error> {
    let current = || gate.is_open() && manager.generation() == generation;
    if !current() {
        return Err(Error::Cancelled);
    }
    let grant = manager.token_with_validity(resource, minimum)?;
    if !current() {
        return Err(Error::Cancelled);
    }
    Ok(grant)
}
