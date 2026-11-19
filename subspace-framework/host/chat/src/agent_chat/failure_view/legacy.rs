use subspace_agent::TurnFailure;
use subspace_agent::failure::TurnContext;
use subspace_core::diagnostics::ErrorDetails;

use super::super::ChatPanel;
use super::super::tabs::Tab;
use subspace_dashboard::copy_details::CopyDetails;

impl ChatPanel {
    pub(in crate::agent_chat) fn copy_legacy_error(&self) -> Option<CopyDetails> {
        let details = if self.tab().error.is_some() {
            &self.tab().errors.legacy
        } else {
            &self.errors.legacy
        };
        details
            .clone()
            .map(|details| CopyDetails::new("chat-error-copy", details))
    }
}

/// Legacy text has no source timestamp. The projection reports observation time.
pub(in crate::agent_chat) fn legacy_error_details(error: &str, tab: &Tab) -> ErrorDetails {
    let mut turn = TurnContext::default();
    turn.begin(0, submitted_text(tab));
    let failure = TurnFailure::rejected("chat_error", error, &turn, None);
    ErrorDetails::new("Copilot error", failure.message, "Copilot runtime")
        .with_code(failure.error_type)
        .with_private_values(submitted_text(tab))
}

pub(super) fn submitted_text(tab: &Tab) -> Vec<String> {
    tab.last_turn.as_ref().map_or_else(Vec::new, |turn| {
        std::iter::once(turn.text.clone())
            .chain(turn.context.iter().map(|context| context.content.clone()))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{legacy_error_details, submitted_text};
    use crate::agent_chat::tabs::{LastTurn, Tab};
    use subspace_core::diagnostics::LocalDiagnostics;

    #[test]
    fn reconnect_stays_secondary_to_sign_in() {
        use crate::action::Intent;
        use crate::agent_chat::view::reconnect_intent;
        assert_eq!(reconnect_intent(true), Intent::Neutral);
        assert_eq!(reconnect_intent(false), Intent::Primary);
    }

    #[test]
    fn legacy_copy_scrubs_the_submission_and_labels_observation_time() {
        let mut tab = Tab::new(1);
        assert!(submitted_text(&tab).is_empty());
        tab.last_turn = Some(LastTurn {
            text: "PRIVATE FIXTURE INPUT".into(),
            context: Vec::new(),
            images: Vec::new(),
        });
        let error = legacy_error_details("Rejected PRIVATE FIXTURE INPUT", &tab);
        let copy = LocalDiagnostics::default().local_copy(Some(&error));
        assert!(copy.markdown().contains("[message text removed]"));
        assert!(!copy.markdown().contains("PRIVATE FIXTURE INPUT"));
        assert!(copy.markdown().contains("Observed at (UTC)"));
        assert!(!copy.markdown().contains("Occurred at (UTC)"));
    }
}
