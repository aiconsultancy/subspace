use crate::project::{Component, LiveState};

/// Results do not invalidate an interaction already started in a native control.
pub(super) fn changed(previous: &LiveState, next: &LiveState) -> bool {
    previous.definition() != next.definition()
        || previous.session_id() != next.session_id()
        || previous.input_revision() != next.input_revision()
        || previous.values() != next.values()
        || next
            .definition()
            .inputs
            .keys()
            .any(|key| previous.options(key) != next.options(key))
        || next
            .definition()
            .components
            .iter()
            .any(|component| enabled_changed(component, previous, next))
}

fn enabled_changed(component: &Component, previous: &LiveState, next: &LiveState) -> bool {
    previous.is_enabled(&component.id) != next.is_enabled(&component.id)
        || component
            .children()
            .iter()
            .any(|child| enabled_changed(child, previous, next))
}
