//! Pure availability and validation policy for the close confirmation.

use subspace_framework::client_github::model::{GhItem, ItemKind, PrState};

pub(crate) fn is_closable(item: &GhItem) -> bool {
    item.kind == ItemKind::Pr && matches!(item.state, PrState::Open | PrState::Draft)
}

pub(super) fn opening_allowed(writing: bool, confirming: bool) -> bool {
    !writing && !confirming
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Submission {
    Ignore,
    Invalid(&'static str),
    Ready,
}

pub(super) fn validate_submission(
    writing: bool,
    composing: bool,
    text: &str,
    connected: bool,
) -> Submission {
    if writing || composing {
        Submission::Ignore
    } else if text.trim().len() > subspace_framework::client_github::validate::MAX_BODY_BYTES {
        Submission::Invalid("Keep the comment within 64 KB.")
    } else if !connected {
        Submission::Invalid("GitHub is unavailable. Check the account connection and try again.")
    } else {
        Submission::Ready
    }
}
