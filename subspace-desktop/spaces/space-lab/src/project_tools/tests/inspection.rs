use serde_json::json;

use super::{fixture, invocation};
use crate::project_tools::tool;

#[tokio::test]
async fn project_inspection_omits_sources_and_explicit_source_cannot_escape() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let inspect = tool(root.path().to_owned(), false);
    let (_, call) = invocation(json!({"action":"project", "id":"demo"}), true);
    let project = inspect.call(call).await.unwrap();
    assert_eq!(project["definition"]["id"], "demo");
    assert!(!project.to_string().contains("print('example')"));
    let (_, call) = invocation(
        json!({"action":"source", "id":"demo", "path":"../outside"}),
        true,
    );
    assert!(inspect.call(call).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn state_readback_uses_root_owned_live_state() {
    use crate::project_service::{InputRevision, shared};
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let service = shared(root.path()).unwrap();
    service.open("demo").unwrap();
    service
        .set_value(
            "demo",
            "count",
            crate::project::InputValue::Number(3.0),
            &InputRevision::new("stopped", 0),
        )
        .unwrap();
    let inspect = tool(root.path().to_owned(), false);
    let (_, call) = invocation(json!({"action":"state", "id":"demo"}), true);
    let state = inspect.call(call).await.unwrap();
    assert_eq!(state["input_revision"], 1);
    assert_eq!(state["values"]["count"], 3.0);
    assert_eq!(state["action_outcomes"], json!([]));
}

#[cfg(not(unix))]
#[tokio::test]
async fn state_readback_fails_closed_without_private_storage_permissions() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let inspect = tool(root.path().to_owned(), false);
    let (_, call) = invocation(json!({"action":"state", "id":"demo"}), true);
    let error = inspect.call(call).await.unwrap_err();
    assert!(error.contains("private storage permissions unavailable"));
}

#[cfg(unix)]
#[tokio::test]
async fn state_keeps_public_text_and_redacts_known_private_values() {
    use crate::private_settings::{Edit, PrivateSettings};
    use crate::project::{InputDeclaration, ProjectDefinition};
    use crate::project_service::shared;
    use std::collections::BTreeMap;
    use subspace_framework::core::Secret;
    let root = tempfile::tempdir().unwrap();
    let service = shared(root.path()).unwrap();
    let mut definition = ProjectDefinition::new("text-demo", "python-data", "main.py");
    definition.inputs.insert(
        "choice".into(),
        InputDeclaration::String {
            default: "test-only-value".into(),
            options: vec!["test-only-value".into(), "public-choice".into()],
        },
    );
    service
        .create(
            &definition,
            BTreeMap::from([("main.py".into(), b"pass".to_vec())]),
        )
        .unwrap();
    PrivateSettings::new(root.path(), "text-demo")
        .unwrap()
        .update(
            0,
            BTreeMap::from([(
                "TOKEN".into(),
                Edit::Replace(Secret::new("test-only-value")),
            )]),
        )
        .unwrap();
    let inspect = tool(root.path().to_owned(), false);
    let (_, call) = invocation(json!({"action":"state", "id":"text-demo"}), true);
    let output = inspect.call(call).await.unwrap();
    assert_eq!(output["values"]["choice"], "[redacted]");
    assert_eq!(output["options"]["choice"][1], "public-choice");
    assert_eq!(output["enabled"]["heading"], true);
    assert!(!output.to_string().contains("test-only-value"));
}

#[cfg(unix)]
#[test]
fn result_readback_preserves_scope_and_time_while_redacting_private_text() {
    use crate::private_settings::{Edit, PrivateSettings};
    use crate::project::{LivePatch, LiveState, ResultUpdate, ResultValue};
    use crate::project_runtime::SessionStatus;
    use crate::project_service::{ProjectPresentation, shared};
    use std::collections::BTreeMap;
    use subspace_framework::core::Secret;
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let service = shared(root.path()).unwrap();
    let working = service.load("demo").unwrap();
    let mut live = LiveState::new(working.definition.clone(), "test-session").unwrap();
    let mut patch = LivePatch::default();
    patch.results.insert(
        "heading".into(),
        ResultUpdate::new(
            "sample-scope",
            123,
            ResultValue::Text {
                text: "public dummy-private-value".into(),
            },
        ),
    );
    live.apply("test-session", 0, patch).unwrap();
    let presentation = ProjectPresentation::new(working, live.clone(), SessionStatus::Running);
    let settings = PrivateSettings::new(root.path(), "demo").unwrap();
    settings
        .update(
            0,
            BTreeMap::from([(
                "TOKEN".into(),
                Edit::Replace(Secret::new("dummy-private-value")),
            )]),
        )
        .unwrap();
    let output = settings
        .redact_readback(crate::project_tools::execute::state(&presentation, &live))
        .unwrap();
    assert_eq!(output["results"]["heading"]["scope"], "sample-scope");
    assert_eq!(output["results"]["heading"]["evaluated_at_ms"], 123);
    assert_eq!(
        output["results"]["heading"]["value"]["text"],
        "public [redacted]"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn settings_inspection_returns_names_without_values() {
    use crate::private_settings::{Edit, PrivateSettings};
    use std::collections::BTreeMap;
    use subspace_framework::core::Secret;
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let settings = PrivateSettings::new(root.path(), "demo").unwrap();
    settings
        .update(
            0,
            BTreeMap::from([(
                "API_KEY".into(),
                Edit::Replace(Secret::new("test-only-value")),
            )]),
        )
        .unwrap();
    let inspect = tool(root.path().to_owned(), false);
    let (_, call) = invocation(json!({"action":"settings_names", "id":"demo"}), true);
    let metadata = inspect.call(call).await.unwrap();
    assert!(metadata.to_string().contains("API_KEY"));
    assert!(!metadata.to_string().contains("test-only-value"));
}
