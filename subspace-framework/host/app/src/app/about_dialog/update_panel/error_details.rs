//! Retain the accepted failure's time and operation, including stale results.

use subspace_core::diagnostics::ErrorDetails;

use super::updates::{UpdateEvent, UpdateState};

/// String-only outcomes do not claim to contain an underlying error chain.
#[derive(Debug, Clone)]
pub(super) struct UpdateFailure {
    pub(super) message: String,
    pub(super) chain: Option<String>,
}

impl From<&str> for UpdateFailure {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_owned(),
            chain: None,
        }
    }
}

fn details(title: &str, failure: &UpdateFailure, stage: &str) -> ErrorDetails {
    let details = ErrorDetails::new(title, &failure.message, stage);
    match &failure.chain {
        Some(chain) => details.with_chain(chain),
        None => details,
    }
}

pub(super) fn accepted_failure(state: &UpdateState, event: &UpdateEvent) -> Option<ErrorDetails> {
    match (state, event) {
        (UpdateState::Checking, UpdateEvent::CheckResult(Err(failure))) => {
            Some(details("Update check failed", failure, "check for updates"))
        }
        (UpdateState::Installing(_), UpdateEvent::InstallResult(Err(failure))) => Some(details(
            "Update installation failed",
            failure,
            "install update",
        )),
        _ => None,
    }
}

pub(super) fn retained_failure(
    state: &UpdateState,
    details: Option<ErrorDetails>,
) -> Option<ErrorDetails> {
    match state {
        UpdateState::Failed(_) => details,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{accepted_failure, retained_failure};
    use crate::app::about_dialog::update_panel::updates::{ReleaseInfo, UpdateEvent, UpdateState};

    #[test]
    fn only_current_check_or_install_errors_create_details() {
        let check = UpdateEvent::CheckResult(Err("Synthetic update check failure".into()));
        let install = UpdateEvent::InstallResult(Err("Synthetic install failure".into()));
        let installing = UpdateState::Installing(ReleaseInfo {
            version: "2.0.0".into(),
            html_url: "https://example.invalid/release".into(),
        });
        assert!(accepted_failure(&UpdateState::Checking, &check).is_some());
        assert!(accepted_failure(&installing, &install).is_some());
        assert!(accepted_failure(&installing, &check).is_none());
        assert!(accepted_failure(&UpdateState::Idle, &install).is_none());
        let details = accepted_failure(&UpdateState::Checking, &check);
        assert!(
            retained_failure(&UpdateState::Failed("fixture".into()), details.clone()).is_some()
        );
        assert!(retained_failure(&UpdateState::Checking, details).is_none());
    }
}
