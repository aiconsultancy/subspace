use super::{client, detail};
use crate::issue_writes::IssueSaveError;
use serde_json::json;
use subspace_framework::client_test_support::Mock;
fn preview() -> serde_json::Value {
    json!({"token":"a".repeat(64),"source":{"id":"project-1","key":"SUB","name":"Fixture"},
        "destination":{"id":"project-2","key":"OPS","name":"Operations"},"issues":[{
            "id":"1","version":1,"oldIdentifier":"SUB-1","newIdentifier":"OPS-7","stateId":"ops-started","stateName":"Started",
            "cycleCleared":false,"milestoneCleared":false,"estimateCleared":false,"parentDetached":false,"archived":false}]})
}
#[test]
fn move_requires_the_preview_token_and_validates_the_exact_result() {
    let mock = Mock::new(|r| {
        if r.path.ends_with("move-preview") {
            (200, preview().to_string())
        } else {
            let mut issue = detail("1");
            issue["identifier"] = json!("OPS-7");
            issue["projectId"] = json!("project-2");
            issue["projectKey"] = json!("OPS");
            (
                200,
                json!({"issue":issue,"moved":preview()["issues"]}).to_string(),
            )
        }
    });
    let api = client(&mock);
    let preview = api.preview_issue_move("1", "project-2", 1).unwrap();
    assert_eq!(
        api.move_issue(&preview).unwrap().issue.fields.identifier,
        "OPS-7"
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    let sent: serde_json::Value = serde_json::from_str(&requests[1].body).unwrap();
    assert_eq!(sent["token"], preview.token);
    assert_eq!(sent["version"], 1);
}
#[test]
fn a_conflict_does_not_resend_or_choose_new_numbers_automatically() {
    let preview = serde_json::from_value(preview()).unwrap();
    let mock = Mock::new(|_| (409, "{}".into()));
    assert!(matches!(
        client(&mock).move_issue(&preview),
        Err(IssueSaveError::Conflict)
    ));
    assert_eq!(mock.requests().len(), 1);
}
#[test]
fn a_nett_503_on_the_confirmation_leaves_the_move_to_be_checked() {
    // The move may have committed behind a gateway's 503: the editor checks
    // it rather than offering a fresh preview as if NETT were only busy.
    let org = "00000000-0000-4000-8000-000000000001";
    let mock = Mock::new(|r| {
        if r.path.ends_with("/desktop/capabilities") {
            (200, json!({"features": ["issueMoves"]}).to_string())
        } else {
            (503, "{}".into())
        }
    });
    let api = crate::Client::nett_loopback(
        &format!("{}/api", mock.base_url()),
        Some(org),
        Some(subspace_framework::core::Secret::new("nett-move-503")),
    )
    .unwrap()
    .without_live();
    let mut preview = preview();
    preview["reviewContext"] = json!({"issueVersion": 1});
    let preview = serde_json::from_value(preview).unwrap();
    assert_eq!(
        api.move_issue(&preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
    let patches = mock
        .requests()
        .iter()
        .filter(|r| r.method == "PATCH")
        .count();
    assert_eq!(patches, 1, "sent once, never again");
}

#[test]
fn a_subspace_503_leaves_the_move_unconfirmed_as_before() {
    // The move may have committed behind the 503, so it is checked, not
    // reported as not done.
    let preview = serde_json::from_value(preview()).unwrap();
    let mock = Mock::new(|_| (503, "{}".into()));
    assert_eq!(
        client(&mock).move_issue(&preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
}
