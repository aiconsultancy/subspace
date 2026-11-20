//! One optional native account per product, from the identity provider the
//! app registers, independent of settings drafts.

use gpui_kit::{App, AppContext as _, Context, Entity, Global};
use std::sync::Arc;
use subspace_account::{Account, Cancel, Error, Provider, ProviderInfo};

mod bootstrap;
mod lease;
pub(crate) use bootstrap::init;
use lease::LeaseGate;
pub use lease::{AccountSession, AccountTokenSource};

/// How an app adds a native account: its identity provider and the trusted
/// adapter it hands the token source to before Spaces start.
pub struct AccountRegistration {
    /// Builds the provider. Its argument is the product's bundle identifier,
    /// a stable namespace for the provider's keychain entries.
    pub provider: fn(&str) -> Option<Arc<dyn Provider>>,
    /// Installs the app's trusted native token consumer, such as an API client.
    pub configure: Option<fn(AccountTokenSource)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Unavailable,
    Idle,
    Starting,
    Waiting,
    Completing,
    Refreshing,
    SigningOut,
    Failed(Error),
}

#[derive(Clone, Copy)]
enum Retry {
    SignIn,
    Restore,
    Reconnect,
    SignOut,
}

pub(crate) struct NativeAccount {
    manager: Option<Arc<dyn Provider>>,
    gate: Arc<LeaseGate>,
    pub provider: ProviderInfo,
    pub account: Option<Account>,
    pub status: Status,
    before_sign_in: Status,
    retry: Retry,
    cancel: Option<Cancel>,
    url: Option<String>,
    /// The user code to confirm in the browser, during device authorisation.
    pub code: Option<String>,
    run: u64,
}

struct Shared(Entity<NativeAccount>);
impl Global for Shared {}

pub(crate) fn shared(cx: &App) -> Option<Entity<NativeAccount>> {
    cx.try_global::<Shared>().map(|v| v.0.clone())
}

/// A product-composition seam for a trusted native service adapter, such as
/// a Space crate an app adds that calls its own API with the account.
// qual:api — the product chooses the allowed native resource, never a webpage.
pub fn account_session(cx: &App) -> Option<AccountSession> {
    let state = shared(cx)?;
    let state = state.read(cx);
    let valid = matches!(state.status, Status::Idle | Status::Refreshing)
        || (matches!(state.status, Status::Failed(_)) && matches!(state.retry, Retry::Reconnect));
    if !valid {
        return None;
    }
    let manager = state.manager.clone()?;
    Some(AccountSession {
        gate: state.gate.clone(),
        generation: manager.generation(),
        account: state.account.clone()?,
        manager,
    })
}

impl NativeAccount {
    pub fn busy(&self) -> bool {
        matches!(
            self.status,
            Status::Starting
                | Status::Waiting
                | Status::Completing
                | Status::Refreshing
                | Status::SigningOut
        )
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(manager) = self.manager.clone() else {
            return;
        };
        self.close_leases(manager.as_ref());
        let generation = manager.generation();
        self.run += 1;
        let run = self.run;
        let cancel = Cancel::default();
        self.retry = Retry::SignIn;
        self.before_sign_in = self.status;
        self.cancel = Some(cancel.clone());
        self.status = Status::Starting;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let starter = manager.clone();
            let started = cx.background_spawn(async move { starter.start() }).await;
            let started = match started {
                Ok(value) => value,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| this.finish(run, Err(error), cx));
                    return;
                }
            };
            let url = started.url.clone();
            let code = started.code.clone();
            let current = this
                .update(cx, |this, cx| {
                    if this.run != run {
                        return false;
                    }
                    this.url = url.clone();
                    this.code = code.clone();
                    // A provider without a browser leg completes headlessly.
                    this.status = if url.is_some() {
                        Status::Waiting
                    } else {
                        Status::Completing
                    };
                    if let Some(url) = &url {
                        cx.open_url(url);
                    }
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !current {
                cancel.cancel();
                return;
            }
            let result = cx
                .background_spawn(
                    async move { manager.finish(started, &cancel, generation).map(Some) },
                )
                .await;
            let _ = this.update(cx, |this, cx| this.finish(run, result, cx));
        })
        .detach();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.cancel
            && !cancel.cancel()
        {
            self.status = Status::Completing;
            cx.notify();
            return;
        }
        self.cancel = None;
        if let Some(manager) = &self.manager {
            manager.invalidate();
        }
        self.gate.open();
        self.run += 1;
        self.url = None;
        self.code = None;
        self.status = self.before_sign_in;
        cx.notify();
    }

    pub fn reopen(&self, cx: &mut Context<Self>) {
        if let Some(url) = &self.url {
            cx.open_url(url);
        }
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        match self.retry {
            Retry::SignIn => self.start(cx),
            Retry::Restore => self.restore(cx),
            Retry::Reconnect => self.reconnect(cx),
            Retry::SignOut => self.sign_out(cx),
        }
    }

    fn restore(&mut self, cx: &mut Context<Self>) {
        self.renew(true, cx);
    }
    pub fn reconnect(&mut self, cx: &mut Context<Self>) {
        self.renew(false, cx);
    }

    fn renew(&mut self, restore: bool, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(manager) = self.manager.clone() else {
            return;
        };
        self.run += 1;
        let run = self.run;
        self.retry = if restore {
            Retry::Restore
        } else {
            Retry::Reconnect
        };
        self.status = Status::Refreshing;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if restore {
                        manager.restore()
                    } else {
                        manager.reconnect()
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| this.finish(run, result, cx));
        })
        .detach();
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(manager) = self.manager.clone() else {
            return;
        };
        self.close_leases(manager.as_ref());
        self.run += 1;
        let run = self.run;
        self.retry = Retry::SignOut;
        self.status = Status::SigningOut;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { manager.sign_out().map(|()| None) })
                .await;
            let _ = this.update(cx, |this, cx| this.finish(run, result, cx));
        })
        .detach();
    }

    fn finish(&mut self, run: u64, result: Result<Option<Account>, Error>, cx: &mut Context<Self>) {
        if run != self.run {
            return;
        }
        let browser = matches!(self.status, Status::Waiting | Status::Completing);
        self.cancel = None;
        self.url = None;
        self.code = None;
        self.reopen_leases(result.is_ok());
        match result {
            Ok(account) => {
                self.account = account;
                self.status = Status::Idle;
                if browser {
                    cx.activate(true);
                }
            }
            Err(Error::Cancelled) => self.status = self.before_sign_in,
            Err(error) => self.status = Status::Failed(error),
        }
        cx.notify();
    }

    /// Before a sign-in or sign-out, refuse leases and supersede in-flight work.
    fn close_leases(&self, manager: &dyn Provider) {
        self.gate.close();
        manager.invalidate();
    }

    /// After a sign-in or sign-out ends, mark a committed account change with
    /// a new generation before any lease can see it, then allow leases again.
    fn reopen_leases(&self, committed: bool) {
        if self.gate.is_open() {
            return;
        }
        if committed && let Some(manager) = &self.manager {
            manager.invalidate();
        }
        self.gate.open();
    }
}

#[cfg(any(test, all(debug_assertions, feature = "capture")))]
mod fixtures;
#[cfg(test)]
mod tests;
#[cfg(any(test, all(debug_assertions, feature = "capture")))]
pub(crate) use fixtures::seed as fixture;
