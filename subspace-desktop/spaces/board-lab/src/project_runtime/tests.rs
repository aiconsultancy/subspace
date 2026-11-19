#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod contained;
mod correlation;
mod framing;

use super::state::RuntimeState;
use crate::project::{
    Component, ComponentKind, InputDeclaration, ProjectDefinition, WorkerEvent, WorkerMessage,
};
use std::collections::BTreeMap;

fn state() -> RuntimeState {
    state_with_changes(None)
}

fn state_with_changes(changes: Option<tokio::sync::watch::Sender<()>>) -> RuntimeState {
    // Protocol tests own this fixture; starter dashboard edits must not change
    // the inputs and actions used to test revision and correlation behavior.
    let mut definition = ProjectDefinition::new("fixture", "python-default", "main.py");
    definition.inputs = BTreeMap::from([
        (
            "amount".into(),
            InputDeclaration::Number {
                default: 5.0,
                min: 0.0,
                max: 10.0,
                step: 1.0,
            },
        ),
        (
            "metric".into(),
            InputDeclaration::String {
                default: "count".into(),
                options: vec!["count".into(), "total".into()],
            },
        ),
    ]);
    definition.actions.push("refresh".into());
    definition.components.push(Component::new(
        "summary",
        "Summary",
        ComponentKind::Text {
            text: "Fixture".into(),
        },
    ));
    RuntimeState::new(definition, "fixture-session", changes).expect("runtime state")
}

pub(super) fn running() -> RuntimeState {
    running_with_changes(None)
}

fn running_with_changes(changes: Option<tokio::sync::watch::Sender<()>>) -> RuntimeState {
    let mut state = state_with_changes(changes);
    let init = state.init().expect("init message");
    state
        .accept(WorkerMessage::new(init.context, WorkerEvent::Ready))
        .expect("ready acknowledgement");
    state
}
