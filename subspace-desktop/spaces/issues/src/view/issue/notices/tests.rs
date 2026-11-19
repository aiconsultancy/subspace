use super::blocked;
use serde_json::json;
use subspace_api_client::model::IssueDetail;

fn detail() -> IssueDetail {
    let mut value = crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
    let mut issue = value["children"]["items"][0].clone();
    issue["kind"] = json!("blockedBy");
    value["relations"] = json!({"items": [issue],
        "nextCursor": "another-page"});
    serde_json::from_value(value).unwrap()
}

#[test]
fn partial_relations_name_loaded_blockers_without_claiming_the_list_is_complete() {
    let mut detail = detail();
    assert_eq!(blocked(&detail), "Blocked by SUB-3 and others.");
    detail.relations.as_mut().unwrap().next_cursor = None;
    assert_eq!(blocked(&detail), "Blocked by SUB-3.");
}

#[test]
fn missing_or_empty_relations_keep_the_generic_blocked_notice() {
    let mut detail = detail();
    detail.relations.as_mut().unwrap().items.clear();
    assert_eq!(blocked(&detail), "This issue is blocked.");
    detail.relations = None;
    assert_eq!(blocked(&detail), "This issue is blocked.");
}
