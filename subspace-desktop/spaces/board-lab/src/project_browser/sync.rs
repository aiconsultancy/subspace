use super::ProjectBrowser;
use crate::project::{Component, LiveState};

impl ProjectBrowser {
    pub(super) fn observe_outcomes(
        &mut self,
        outcomes: Vec<crate::project_service::ActionOutcome>,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let unseen = outcomes
            .iter()
            .filter(|outcome| {
                !self
                    .seen_outcomes
                    .contains(&(outcome.session_id.clone(), outcome.request_id.clone()))
            })
            .collect::<Vec<_>>();
        let errors = unseen
            .iter()
            .filter_map(|outcome| {
                outcome.error.as_ref().map(|error| {
                    format!(
                        "Session {} · action {} failed: {error}",
                        outcome.session_id, outcome.request_id
                    )
                })
            })
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            self.error = Some(errors.join("\n"));
        } else if let Some(latest) = unseen.last() {
            self.status = format!(
                "Session {} · action {} completed",
                latest.session_id, latest.request_id
            );
        }
        self.seen_outcomes = outcomes
            .into_iter()
            .map(|outcome| (outcome.session_id, outcome.request_id))
            .collect();
        cx.notify();
    }
}

/// An unchanged observation must not reset an open select or its event origin.
pub(super) fn same_live(left: &LiveState, right: &LiveState) -> bool {
    left.session_id() == right.session_id()
        && left.input_revision() == right.input_revision()
        && left.definition() == right.definition()
        && left.values() == right.values()
        && left.results() == right.results()
        && left
            .definition()
            .inputs
            .keys()
            .all(|key| left.options(key) == right.options(key))
        && same_enabled(&left.definition().components, left, right)
}

fn same_enabled(components: &[Component], left: &LiveState, right: &LiveState) -> bool {
    components.iter().all(|component| {
        left.is_enabled(&component.id) == right.is_enabled(&component.id)
            && same_enabled(component.children(), left, right)
    })
}
