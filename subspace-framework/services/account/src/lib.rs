//! Provider-neutral native account sessions.
//!
//! An app chooses one identity provider (for example Microsoft Entra, WorkOS
//! AuthKit or a local development issuer) and hands the shell an
//! `Arc<dyn Provider>`. The shell owns the account UI and silent renewal;
//! trusted native service adapters ask the provider for resource tokens.
//! No client secret, browser-cookie copying, plaintext vault or provider
//! error text ever reaches UI state.

pub mod loopback;
mod store;

pub use store::{CredentialStore, MemoryStore, SystemStore};

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subspace_core::Secret;

/// The signed-in identity. Display labels never authorise a request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Account {
    /// The issuer-scoped organisation or tenant the identity belongs to.
    pub tenant: String,
    /// The provider's immutable identifier for the user within `tenant`.
    pub id: String,
    pub name: String,
    pub username: String,
}

/// A resource-specific credential for trusted native service adapters only.
/// Never pass it into JavaScript, configuration, navigation URLs or UI state.
pub struct ServiceToken {
    pub account: Account,
    pub token: Secret,
    pub expires_at: Instant,
}

/// Fixed, provider-neutral messages: they are shown as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Sign-in is not configured in this build.")]
    Unconfigured,
    #[error("Sign-in was cancelled. Your account has not changed.")]
    Cancelled,
    #[error("Sign-in took too long. Choose Sign in again.")]
    TimedOut,
    #[error("The sign-in service could not be reached. Check your connection and choose Retry.")]
    Offline,
    #[error("Your session needs attention. Choose Sign in again to continue.")]
    SignInRequired,
    #[error("The sign-in service did not allow this sign-in. Choose another account.")]
    NotPermitted,
    #[error("The sign-in reply could not be verified. Choose Sign in again.")]
    InvalidReply,
    #[error(
        "The app could not open a local sign-in callback. Close the other attempt and try again."
    )]
    Listener,
    #[error("The app cannot access your system keychain. Unlock it, then choose Retry.")]
    Keychain,
    #[error("Sign-in could not be completed. Try again.")]
    Service,
}

/// Cooperative cancellation with one atomic boundary: once a provider starts
/// its credential-store transaction, cancellation can no longer win.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicU8>);

const ACTIVE: u8 = 0;
const CANCELLED: u8 = 1;
const COMMITTING: u8 = 2;

impl Cancel {
    /// Once the credential-store transaction starts, let it finish. A caller
    /// must keep showing progress instead of claiming that it was cancelled.
    pub fn cancel(&self) -> bool {
        match self
            .0
            .compare_exchange(ACTIVE, CANCELLED, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) | Err(CANCELLED) => true,
            Err(_) => false,
        }
    }

    pub fn check(&self) -> Result<(), Error> {
        if self.0.load(Ordering::SeqCst) == CANCELLED {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Called by a provider immediately before it persists a new credential.
    pub fn begin_commit(&self) -> Result<(), Error> {
        self.0
            .compare_exchange(ACTIVE, COMMITTING, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| Error::Cancelled)
    }
}

/// An interactive sign-in in progress. `url`, when present, is the prompt the
/// shell opens in the system browser. `code`, when present, is a short
/// user code the person confirms on that page, as in device authorisation;
/// it is not a secret. Device codes, verifiers and tokens stay inside the
/// provider, under `ticket`.
pub struct Started {
    pub url: Option<String>,
    pub code: Option<String>,
    ticket: Ticket,
}

struct Ticket {
    id: u64,
    release: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl Started {
    /// A sign-in with no browser leg and no pending state.
    pub fn headless() -> Self {
        Self {
            url: None,
            code: None,
            ticket: Ticket {
                id: 0,
                release: None,
            },
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

/// A provider's one in-flight interactive sign-in. A newer start replaces an
/// older one, whose `finish` then reports it as cancelled.
pub struct PendingSlot<T> {
    slot: Arc<Mutex<Option<(u64, T)>>>,
}

static NEXT_TICKET: AtomicU64 = AtomicU64::new(1);

impl<T> Default for PendingSlot<T> {
    fn default() -> Self {
        Self {
            slot: Arc::default(),
        }
    }
}

impl<T> PendingSlot<T> {
    /// Keep `pending` and hand the shell its prompt.
    /// Dropping the prompt without finishing releases that attempt's resources.
    pub fn start(&self, url: Option<String>, pending: T) -> Result<Started, Error>
    where
        T: Send + 'static,
    {
        self.start_with_code(url, None, pending)
    }

    /// As [`PendingSlot::start`], with a user code for the shell to show
    /// beside the browser prompt.
    pub fn start_with_code(
        &self,
        url: Option<String>,
        code: Option<String>,
        pending: T,
    ) -> Result<Started, Error>
    where
        T: Send + 'static,
    {
        let ticket = NEXT_TICKET.fetch_add(1, Ordering::Relaxed);
        *self.slot.lock().map_err(|_| Error::Service)? = Some((ticket, pending));
        let slot = Arc::downgrade(&self.slot);
        let release = move || {
            if let Some(slot) = slot.upgrade()
                && let Ok(mut slot) = slot.lock()
                && slot.as_ref().is_some_and(|(current, _)| *current == ticket)
            {
                drop(slot.take());
            }
        };
        Ok(Started {
            url,
            code,
            ticket: Ticket {
                id: ticket,
                release: Some(Box::new(release)),
            },
        })
    }

    /// The pending state `started` names, once.
    pub fn take(&self, started: &Started) -> Result<T, Error> {
        let mut slot = self.slot.lock().map_err(|_| Error::Service)?;
        match slot.take() {
            Some((ticket, pending)) if ticket == started.ticket.id => Ok(pending),
            other => {
                *slot = other;
                Err(Error::Cancelled)
            }
        }
    }
}

/// Labels the account settings show. None of them is a secret.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderInfo {
    /// The identity provider's name, such as "Microsoft" or "WorkOS".
    pub name: String,
    /// The organisation the deployment belongs to, when the app names one.
    pub organization: String,
    /// A public detail worth showing, such as a tenant ID, with its label.
    pub detail: Option<(String, String)>,
    /// Whether sign-in opens the system browser.
    pub browser: bool,
}

/// One native identity provider. Implementations serialise renewal, keep
/// refresh credentials only in a [`CredentialStore`] and access tokens only
/// in memory, and never open a browser outside [`Provider::start`]. Every
/// method that may touch the network belongs on a background thread.
pub trait Provider: Send + Sync {
    fn info(&self) -> ProviderInfo;
    /// Changes whenever the account may have changed, including when
    /// [`Provider::finish`] commits a new identity; a token obtained under an
    /// older generation must not be used.
    fn generation(&self) -> u64;
    /// Supersede in-flight work, before a new sign-in or sign-out.
    fn invalidate(&self);
    /// Begin an interactive sign-in.
    fn start(&self) -> Result<Started, Error>;
    /// Complete it: accept and persist before the browser can display success,
    /// advancing the generation in the same step that installs the identity.
    fn finish(&self, started: Started, cancel: &Cancel, generation: u64) -> Result<Account, Error>;
    /// Silent startup restoration; no stored credential is signed out.
    fn restore(&self) -> Result<Option<Account>, Error>;
    /// Renew the account without ever opening a browser.
    fn reconnect(&self) -> Result<Option<Account>, Error>;
    /// Forget this app's session. Callers must report failures.
    fn sign_out(&self) -> Result<(), Error>;
    /// A token for a resource the app registered with the provider, renewed
    /// silently when less than `minimum` of its lifetime remains.
    fn token_with_validity(&self, resource: &str, minimum: Duration)
    -> Result<ServiceToken, Error>;
    /// Whether every identity is a disposable local development one: no
    /// browser, no stored credential and only a loopback service. Debug
    /// capture builds sign in only with such a provider.
    fn local_development(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{Cancel, Error, PendingSlot, Started};
    use std::sync::Arc;

    #[test]
    fn cancellation_and_credential_acceptance_have_one_atomic_boundary() {
        let cancelled = Cancel::default();
        assert!(cancelled.cancel());
        assert_eq!(cancelled.begin_commit(), Err(Error::Cancelled));
        let committing = Cancel::default();
        assert_eq!(committing.begin_commit(), Ok(()));
        assert!(
            !committing.cancel(),
            "the UI must wait for the credential transaction result"
        );
        assert_eq!(committing.check(), Ok(()));
    }

    #[test]
    fn only_the_latest_sign_in_reads_back_its_pending_value_once() {
        let slot = PendingSlot::default();
        let first = slot.start(None, 1_u32).unwrap();
        let other_slot = PendingSlot::default();
        let other = other_slot.start(None, 3_u32).unwrap();
        assert_eq!(other_slot.take(&first), Err(Error::Cancelled));
        assert_eq!(other_slot.take(&other), Ok(3));
        let second = slot
            .start(Some("https://example.com".into()), 2_u32)
            .unwrap();
        assert_eq!(slot.take(&first), Err(Error::Cancelled));
        assert_eq!(slot.take(&second), Ok(2));
        assert_eq!(slot.take(&second), Err(Error::Cancelled));
        assert!(slot.take(&Started::headless()).is_err());
        let coded = slot
            .start_with_code(None, Some("ABCD-EFGH".into()), 4_u32)
            .unwrap();
        assert_eq!(coded.code.as_deref(), Some("ABCD-EFGH"));
        assert_eq!(slot.take(&coded), Ok(4));
        assert_eq!(Started::headless().code, None);
    }

    #[test]
    fn dropping_a_browser_prompt_releases_only_its_own_pending_state() {
        let slot = PendingSlot::default();
        let value = Arc::new(());
        let first = slot.start(None, value.clone()).unwrap();
        let second = slot.start(None, value.clone()).unwrap();
        drop(first);
        assert_eq!(Arc::strong_count(&value), 2);
        drop(second);
        assert_eq!(Arc::strong_count(&value), 1);
    }
}
