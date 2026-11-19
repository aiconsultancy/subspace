//! Sign in with GitHub through the app's OAuth App, without GPUI.
//!
//! The app supplies its OAuth App ([`OAuthClient`]); the Framework has none.
//! With the client secret, sign-in uses the browser: an authorization
//! code with PKCE (RFC 7636) and a random `state`, returned to a one-shot
//! listener on `127.0.0.1` ([`browser`]). Without it, sign-in uses the device
//! flow (RFC 8628, [`device`]). Either way the token's owner comes from
//! `GET /user`. Tokens, codes and the secret are [`Secret`]s or stay inside
//! this crate; no log line or error carries one.

pub mod browser;
pub mod config;
pub mod device;
pub mod pkce;
pub mod token;
mod user;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use subspace_core::Secret;

pub use config::{Endpoints, OAuthClient};

/// How long a sign-in waits for the browser or the code before giving up.
pub const SIGN_IN_TIME_LIMIT: Duration = Duration::from_secs(5 * 60);

/// Why a sign-in did not finish. Every message is plain words for the user
/// and carries no token, code or secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// The user cancelled in Subspace.
    #[error("Sign-in was cancelled.")]
    Cancelled,
    #[error("GitHub sign-in took too long. Try again.")]
    TimedOut,
    #[error("The browser's reply did not belong to this sign-in. Try again.")]
    StateMismatch,
    #[error("GitHub sign-in was declined in the browser.")]
    Denied,
    #[error("The code expired before it was entered on GitHub. Try again.")]
    Expired,
    #[error(
        "GitHub does not allow sign-in with a code for this app yet. Use a release build of \
         the app, or run `gh auth login`."
    )]
    DeviceFlowDisabled,
    #[error("GitHub did not grant {0}. Sign in again and approve every permission.")]
    MissingScopes(String),
    #[error("Subspace cannot wait for the browser: {0}.")]
    Listener(String),
    #[error("GitHub could not complete the sign-in: {0}")]
    Service(String),
    #[error("GitHub could not be reached: {0}")]
    Network(String),
}

/// A sign-in's cancel switch, shared with the thread that waits.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// What the user does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// Continue on GitHub in the browser, opened at `url`.
    Browser { url: String },
    /// Enter `user_code` at `verification_uri`.
    Device {
        user_code: String,
        verification_uri: String,
    },
}

/// A finished sign-in: the account and its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedIn {
    pub login: String,
    pub token: Secret,
}

/// The rest of a started sign-in, run off the UI thread.
pub trait Pending: Send {
    /// Waits for the browser or the code, then exchanges and checks the
    /// token. Returns [`AuthError::Cancelled`] soon after `cancel`.
    fn finish(self: Box<Self>, cancel: &Cancel) -> Result<SignedIn, AuthError>;
}

/// A started sign-in: what to show, and what to wait for.
pub struct Started {
    pub prompt: Prompt,
    pub pending: Box<dyn Pending>,
}

/// Starts sign-ins and revokes signed-out tokens; injectable for tests.
pub trait Authenticator: Send + Sync {
    fn start(&self) -> Result<Started, AuthError>;
    /// Best effort: asks GitHub to forget the app's grant for `token`.
    fn revoke(&self, token: &Secret);
}

/// Sign-in through the app's GitHub OAuth App.
#[derive(Debug, Clone)]
pub struct GitHubOAuth {
    client: OAuthClient,
    endpoints: Endpoints,
    time_limit: Duration,
}

impl GitHubOAuth {
    /// `client`, with the build's endpoints.
    pub fn for_client(client: OAuthClient) -> Self {
        Self::new(client, Endpoints::from_build())
    }

    pub fn new(client: OAuthClient, endpoints: Endpoints) -> Self {
        Self {
            client,
            endpoints,
            time_limit: SIGN_IN_TIME_LIMIT,
        }
    }

    /// A shorter wait, for tests.
    // qual:api
    pub fn with_time_limit(mut self, time_limit: Duration) -> Self {
        self.time_limit = time_limit;
        self
    }
}

impl Authenticator for GitHubOAuth {
    fn start(&self) -> Result<Started, AuthError> {
        match &self.client.secret {
            Some(secret) => {
                browser::start(&self.client.id, secret, &self.endpoints, self.time_limit)
            }
            None => device::start(&self.client.id, &self.endpoints, self.time_limit),
        }
    }

    fn revoke(&self, token: &Secret) {
        if let Some(secret) = &self.client.secret {
            user::revoke(&self.endpoints, (&self.client.id, secret), token);
        }
    }
}

/// The account a token belongs to, once its scopes are known to be enough.
pub(crate) fn signed_in(
    endpoints: &Endpoints,
    granted: token::Granted,
) -> Result<SignedIn, AuthError> {
    token::check_scopes(&granted.scopes)?;
    let login = user::login(endpoints, &granted.token)?;
    Ok(SignedIn {
        login,
        token: granted.token,
    })
}

/// Blocks for `duration` in short steps, returning early on `cancel`.
pub(crate) fn sleep(duration: Duration, cancel: &Cancel) -> Result<(), AuthError> {
    const STEP: Duration = Duration::from_millis(100);
    let mut left = duration;
    while !left.is_zero() {
        if cancel.is_cancelled() {
            return Err(AuthError::Cancelled);
        }
        let step = left.min(STEP);
        std::thread::sleep(step);
        left -= step;
    }
    Ok(())
}
