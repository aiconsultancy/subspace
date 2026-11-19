use serde_json::json;
use subspace_framework::agent_tools::{Access, ApprovalPolicy};

use super::{fixture, invocation};
use crate::project::ProjectRevision;
use crate::project_service::{InputRevision, shared};
use crate::project_tools::tool;

#[test]
fn catalogs_declare_action_arguments_and_approval_policies() {
    let root = tempfile::tempdir().unwrap();
    let inspect = tool(root.path().to_owned(), false);
    let edit = tool(root.path().to_owned(), true);
    assert_eq!(
        (inspect.spec().access, inspect.spec().approval),
        (Access::Read, ApprovalPolicy::None)
    );
    assert_eq!(
        (edit.spec().access, edit.spec().approval),
        (Access::Write, ApprovalPolicy::PerCall)
    );
    let schema = edit.spec().input_schema();
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"]["expected"].is_object());
    assert!(schema["properties"].get("secret").is_none());
}

#[tokio::test]
async fn execution_approval_has_revision_and_authority_but_no_source() {
    let root = tempfile::tempdir().unwrap();
    let revision = fixture(root.path());
    let edit = tool(root.path().to_owned(), true);
    let (decision, call) = invocation(
        json!({"action":"start", "id":"demo", "expected":revision}),
        false,
    );
    assert!(edit.call(call).await.is_err());
    let summaries = decision.summaries.lock().unwrap();
    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];
    for expected in [
        revision.definition.as_str(),
        revision.source.as_str(),
        "API_KEY",
        "https://api.example.com",
        "python-data",
    ] {
        assert!(summary.contains(expected), "missing authority in approval");
    }
    assert!(!summary.contains("print('example')"));
    assert!(
        shared(root.path())
            .unwrap()
            .open("demo")
            .unwrap()
            .applied_revision
            .is_none()
    );
}

#[tokio::test]
async fn stale_start_and_secret_arguments_are_rejected_before_approval() {
    let root = tempfile::tempdir().unwrap();
    let revision = fixture(root.path());
    let edit = tool(root.path().to_owned(), true);
    let stale = ProjectRevision::new("stale", revision.source.clone());
    let (decision, call) = invocation(
        json!({"action":"start", "id":"demo", "expected":stale}),
        true,
    );
    assert!(edit.call(call).await.is_err());
    assert!(decision.summaries.lock().unwrap().is_empty());
    let (decision, call) = invocation(
        json!({"action":"start", "id":"demo", "expected":revision, "secret":"dummy"}),
        true,
    );
    assert!(edit.call(call).await.is_err());
    assert!(decision.summaries.lock().unwrap().is_empty());
}

#[tokio::test]
async fn input_action_rejects_wrong_session_even_at_same_revision() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let service = shared(root.path()).unwrap();
    let initial = service.open("demo").unwrap();
    let rejected = service.set_value(
        "demo",
        "count",
        crate::project::InputValue::Number(3.0),
        &InputRevision::new("other-session", initial.live.input_revision()),
    );
    assert!(
        rejected
            .unwrap_err()
            .to_string()
            .contains("revision conflict")
    );
    let edit = tool(root.path().to_owned(), true);
    let arguments = json!({"action":"set_value", "id":"demo", "key":"count", "value":3.0,
        "expected":{"session_id":"other-session", "input_revision":initial.live.input_revision()}});
    let (_, call) = invocation(arguments, true);
    assert!(edit.call(call).await.is_err());
    let unchanged = service.open("demo").unwrap();
    assert_eq!(
        unchanged.live.input_revision(),
        initial.live.input_revision()
    );
    assert_eq!(
        unchanged.live.values()["count"],
        initial.live.values()["count"]
    );
    let arguments = json!({"action":"set_value", "id":"demo", "key":"count", "value":3.0,
        "expected":{"session_id":initial.live.session_id(), "input_revision":initial.live.input_revision()}});
    let (_, call) = invocation(arguments, true);
    edit.call(call).await.unwrap();
    assert_eq!(service.open("demo").unwrap().live.input_revision(), 1);
}

#[tokio::test]
async fn stale_edit_cannot_replace_source() {
    let root = tempfile::tempdir().unwrap();
    let revision = fixture(root.path());
    let service = shared(root.path()).unwrap();
    service
        .edit_file("demo", "main.py", b"new source", &revision)
        .unwrap();
    let edit = tool(root.path().to_owned(), true);
    let (_, call) = invocation(
        json!({"action":"edit_file", "id":"demo", "path":"main.py", "source":"overwrite", "expected":revision}),
        true,
    );
    assert!(edit.call(call).await.is_err());
    assert_eq!(
        service.load("demo").unwrap().files["main.py"],
        b"new source"
    );
}
