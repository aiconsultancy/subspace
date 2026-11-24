//! What the account says and when the app asks for sign-in, from its
//! status alone, so both are tested without a provider.

use subspace_account::Error;

use super::{Retry, Status};

/// Whether the person has no usable session: `status` now and the status
/// before the sign-in in progress, if one is, how the last failure would be
/// retried, and whether an account is known. A saved session that cannot
/// be renewed because the service, the network or the keychain is out of
/// reach still counts as one, so the app opens with what it has.
pub(super) fn needs_session(
    (status, before): (Status, Status),
    retry: Retry,
    signed_in: bool,
) -> bool {
    match status {
        Status::Starting | Status::Waiting | Status::Completing => settled_need(before, signed_in),
        Status::Failed(_) if matches!(retry, Retry::SignIn) => {
            settled_need(before, signed_in) || settled_need(status, signed_in)
        }
        _ => settled_need(status, signed_in),
    }
}

/// [`needs_session`] for a status that is not a sign-in in progress.
fn settled_need(status: Status, signed_in: bool) -> bool {
    match status {
        Status::Idle => !signed_in,
        Status::Failed(Error::SignInRequired | Error::InvalidReply) => true,
        Status::Failed(Error::Offline | Error::Keychain | Error::Service) => false,
        Status::Failed(_) => !signed_in,
        _ => false,
    }
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
