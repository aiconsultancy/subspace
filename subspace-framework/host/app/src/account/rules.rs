//! What the account says and when the app asks for sign-in, from its
//! status and session alone, so both are tested without a provider.

use subspace_account::Error;

use super::{Retry, Status};

/// What the app knows of the person's session, apart from the outcome of
/// the last sign-in attempt, which never changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Session {
    /// Still restoring the saved session at start-up.
    Unknown,
    /// Signed in, restored or renewed.
    Usable,
    /// A saved session that could not be checked because the service, the
    /// network or the keychain was out of reach: it still lets the person in.
    Unchecked,
    /// None: none was saved, the person signed out, or it can no longer be
    /// renewed.
    None,
}

/// The session after `result` of the work `retry` names. Only restoring or
/// renewing a saved session, signing in and signing out change it; a
/// failed, timed-out or cancelled sign-in attempt leaves it as it was.
pub(super) fn after(session: Session, retry: Retry, result: Result<bool, Error>) -> Session {
    match (retry, result) {
        (_, Ok(true)) => Session::Usable,
        (_, Ok(false)) => Session::None,
        (
            Retry::Restore | Retry::Reconnect,
            Err(Error::Offline | Error::Keychain | Error::Service),
        ) if session != Session::None => Session::Unchecked,
        (Retry::Restore | Retry::Reconnect, Err(Error::SignInRequired | Error::InvalidReply)) => {
            Session::None
        }
        (Retry::Restore, Err(_)) if session == Session::Unknown => Session::None,
        _ => session,
    }
}

/// Whether the app asks for sign-in: no session, and sign-in available.
pub(super) fn asks(session: Session, status: Status) -> bool {
    session == Session::None && status != Status::Unavailable
}

/// Whether routes from outside the window wait: while it asks for sign-in,
/// or may yet once the saved session is restored.
pub(super) fn holds(session: Session, status: Status) -> bool {
    matches!(session, Session::Unknown | Session::None) && status != Status::Unavailable
}

/// The account's heading: the organisation's account, else the provider's.
pub(crate) fn account_label(organization: &str, provider: &str) -> String {
    match (organization.is_empty(), provider.is_empty()) {
        (false, _) => format!("{organization} account"),
        (true, false) => format!("{provider} account"),
        (true, true) => "Account".into(),
    }
}

/// What the account is doing, when it says anything: `code` is the user
/// code to confirm during device authorisation.
pub(crate) fn status_text(
    status: Status,
    signed_in: bool,
    provider: &str,
    code: Option<&str>,
) -> Option<String> {
    match status {
        Status::Starting => Some(format!("Opening {provider} sign-in…")),
        Status::Waiting if code.is_some() => code.map(|code| {
            format!("Check that your browser shows the code {code}, then confirm it there.")
        }),
        Status::Waiting if signed_in => Some("Complete sign-in in your browser. Your current account stays connected until this finishes.".into()),
        Status::Waiting => Some("Complete sign-in in your browser.".into()),
        Status::Refreshing => Some(format!("Reconnecting to your {provider} account…")),
        Status::Completing => Some(format!("Saving your {provider} sign-in securely…")),
        Status::SigningOut => Some("Signing out…".into()),
        Status::Failed(error) => Some(error.to_string()),
        _ => None,
    }
}

/// The sign-in action's label.
pub(crate) fn sign_in_label(status: Status, signed_in: bool, provider: &str) -> String {
    if matches!(
        status,
        Status::Failed(Error::SignInRequired | Error::InvalidReply)
    ) {
        "Sign in again".into()
    } else if signed_in {
        "Switch account".into()
    } else if provider.is_empty() {
        "Sign in".into()
    } else {
        format!("Sign in with {provider}")
    }
}
