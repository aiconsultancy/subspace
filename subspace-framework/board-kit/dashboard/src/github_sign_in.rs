//! Sign in with GitHub, shared by Settings and chat: the one sign-in in
//! progress ([`GitHubSignIn`]) and the component that shows it
//! ([`status`]). The flow itself is headless (`subspace-github-auth`); the
//! token goes straight from it into the shared credentials' store, never
//! into UI state. Sign-in and sign-out apply at once: they are not Settings
//! drafts.

use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global};
use subspace_core::credentials::Credentials;
use subspace_github_auth::{
    AuthError, Authenticator, Cancel, GitHubOAuth, OAuthClient, Prompt, Started,
};

use crate::Services;

/// Where a sign-in stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// Asking GitHub how to sign in.
    Starting,
    /// Waiting for the user to approve in the browser.
    Browser,
    /// Waiting for the user to enter `user_code` at `verification_uri`.
    Device {
        user_code: String,
        verification_uri: String,
    },
    /// The last attempt failed, in plain words.
    Failed(String),
}

/// What changed an account, for views that list accounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInEvent {
    SignedIn(String),
    SignedOut(String),
}

/// How one sign-in ended.
enum Outcome {
    SignedIn(String),
    Cancelled,
    Failed(String),
}

impl From<AuthError> for Outcome {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::Cancelled => Self::Cancelled,
            error => Self::Failed(error.to_string()),
        }
    }
}

/// The app's one sign-in.
pub struct GitHubSignIn {
    authenticator: Arc<dyn Authenticator>,
    phase: Phase,
    cancel: Option<Cancel>,
    /// Bumped by each start and cancel, so a stale result is dropped.
    run: u64,
}

impl EventEmitter<SignInEvent> for GitHubSignIn {}

struct Shared(Entity<GitHubSignIn>);

impl Global for Shared {}

/// Installs sign-in through the app's OAuth App (the browser with its
/// client secret, else the device flow). Without one, nothing is installed,
/// [`shared`] is `None` and no view offers Sign in with GitHub.
pub fn init(client: Option<OAuthClient>, cx: &mut App) {
    if let Some(client) = client {
        install(Arc::new(GitHubOAuth::for_client(client)), cx);
    }
}

/// Installs a sign-in through `authenticator`, such as a test fake.
pub fn install(authenticator: Arc<dyn Authenticator>, cx: &mut App) -> Entity<GitHubSignIn> {
    let entity = cx.new(|_| GitHubSignIn::new(authenticator));
    cx.set_global(Shared(entity.clone()));
    entity
}

/// The installed sign-in, if any.
pub fn shared(cx: &App) -> Option<Entity<GitHubSignIn>> {
    cx.try_global::<Shared>().map(|shared| shared.0.clone())
}

impl GitHubSignIn {
    pub fn new(authenticator: Arc<dyn Authenticator>) -> Self {
        Self {
            authenticator,
            phase: Phase::Idle,
            cancel: None,
            run: 0,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn in_progress(&self) -> bool {
        matches!(
            self.phase,
            Phase::Starting | Phase::Browser | Phase::Device { .. }
        )
    }

    /// Starts a sign-in unless one is running. The browser opens by itself;
    /// a device code waits for the user to open GitHub.
    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.in_progress() {
            return;
        }
        let Some(credentials) = credentials(cx) else {
            self.set_phase(Phase::Failed("Sign-in is unavailable here.".into()), cx);
            return;
        };
        let cancel = Cancel::default();
        self.run += 1;
        self.cancel = Some(cancel.clone());
        self.set_phase(Phase::Starting, cx);
        let run = self.run;
        let authenticator = self.authenticator.clone();
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_spawn(async move { authenticator.start() })
                .await;
            let started = match started {
                Ok(started) => started,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| this.finish(run, error.into(), cx));
                    return;
                }
            };
            let Started { prompt, pending } = started;
            let _ = this.update(cx, |this, cx| this.prompt(run, prompt, cx));
            let outcome = cx
                .background_spawn(async move {
                    let signed_in = pending.finish(&cancel)?;
                    keep(&credentials, &cancel, &signed_in.login, &signed_in.token)
                })
                .await
                .unwrap_or_else(Outcome::from);
            let _ = this.update(cx, |this, cx| this.finish(run, outcome, cx));
        })
        .detach();
    }

    /// Stops the running sign-in; nothing is kept.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.run += 1;
        self.set_phase(Phase::Idle, cx);
    }

    /// Forgets `login`'s sign-in at once and asks GitHub to revoke its
    /// token. gh's own account of the same login is not changed.
    pub fn sign_out(&mut self, login: String, cx: &mut Context<Self>) {
        let Some(credentials) = credentials(cx) else {
            return;
        };
        let authenticator = self.authenticator.clone();
        cx.spawn(async move |this, cx| {
            let account = login.clone();
            let removed = cx
                .background_spawn(async move {
                    let token = credentials.sign_out(&account)?;
                    if let Some(token) = token {
                        authenticator.revoke(&token);
                    }
                    Ok::<(), subspace_core::credentials::CredentialError>(())
                })
                .await;
            let _ = this.update(cx, |this, cx| match removed {
                Ok(()) => cx.emit(SignInEvent::SignedOut(login)),
                Err(error) => this.set_phase(Phase::Failed(error.0), cx),
            });
        })
        .detach();
    }

    /// Shows a phase as it is, for previews and tests.
    pub fn show(&mut self, phase: Phase, cx: &mut Context<Self>) {
        self.set_phase(phase, cx);
    }

    fn set_phase(&mut self, phase: Phase, cx: &mut Context<Self>) {
        self.phase = phase;
        cx.notify();
    }

    fn prompt(&mut self, run: u64, prompt: Prompt, cx: &mut Context<Self>) {
        if run != self.run {
            return;
        }
        let phase = match prompt {
            Prompt::Browser { url } => {
                cx.open_url(&url);
                Phase::Browser
            }
            Prompt::Device {
                user_code,
                verification_uri,
            } => Phase::Device {
                user_code,
                verification_uri,
            },
        };
        self.set_phase(phase, cx);
    }

    fn finish(&mut self, run: u64, outcome: Outcome, cx: &mut Context<Self>) {
        if run != self.run {
            return;
        }
        let from_browser = self.phase == Phase::Browser;
        self.cancel = None;
        match outcome {
            Outcome::SignedIn(login) => {
                self.set_phase(Phase::Idle, cx);
                // The browser took the user away; bring Subspace back.
                if from_browser {
                    cx.activate(true);
                }
                cx.emit(SignInEvent::SignedIn(login));
            }
            Outcome::Cancelled => self.set_phase(Phase::Idle, cx),
            Outcome::Failed(message) => self.set_phase(Phase::Failed(message), cx),
        }
    }
}

fn credentials(cx: &App) -> Option<Arc<Credentials>> {
    cx.try_global::<Services>()
        .map(|services| services.credentials.clone())
}

/// Saves a finished sign-in unless it was cancelled meanwhile.
fn keep(
    credentials: &Credentials,
    cancel: &Cancel,
    login: &str,
    token: &subspace_core::Secret,
) -> Result<Outcome, AuthError> {
    if cancel.is_cancelled() {
        return Err(AuthError::Cancelled);
    }
    Ok(match credentials.sign_in(login, token) {
        Ok(()) => Outcome::SignedIn(login.to_owned()),
        Err(error) => Outcome::Failed(error.0),
    })
}

mod view;

pub use view::{sign_in_button, status};

#[cfg(test)]
mod tests;
