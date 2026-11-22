use crate::tests::offline::{fixture_hub, offline_with, row, wait_for};
use serde_json::json;

fn resolved() -> subspace_framework::contracts::issues::IssueSummary {
    let fixture = offline_with(|request| {
        if request.path != "/api/v1/issues/SUB-1" {
            return None;
        }
        let mut detail = row("1", "Fixture tile");
        detail["type"] = json!("tracker");
        detail["todos"] = json!([]);
        detail["assignee"] = json!({"userId":"agent", "displayName":"protostar", "kind":"agent", "isFormerMember":false, "agent":{"rank":"captain", "fleet":"fixture-fleet", "ship":"fixture-ship"}});
        detail["owner"] = json!({"userId":"owner", "displayName":"Rosa Fixture", "kind":"human", "isFormerMember":false});
        detail["progress"] =
            json!({"done":3,"total":7,"canceled":0,"duplicate":0,"sample":[],"hasMore":false});
        detail["pullRequestSummary"] = json!({"total":5,"open":1,"closed":0,"merged":4,"closingPending":0,"checksState":"success"});
        detail["dueDate"] = json!("2026-11-20");
        detail["readyForAgents"] = json!(true);
        detail["lastActivity"] = json!({"kind":"comment.created", "occurredAt":"2026-11-16T12:00:00Z", "actor":{"type":"agent","userId":"agent","label":"protostar"}});
        Some((200, detail.to_string()))
    });
    let (hub, _) = fixture_hub(&fixture);
    wait_for(|| !hub.irc_snapshot().project_keys().is_empty());
    hub.resolve_reference("SUB-1", "subspace-api");
    wait_for(|| !hub.irc_snapshot().cards().is_empty());
    let snapshot = hub.irc_snapshot();
    let summary = super::from_card(&snapshot, snapshot.cards()[0].card().unwrap());
    hub.stop();
    summary
}
#[test]
fn api_card_preserves_optional_tile_facts_through_the_responder() {
    let issue = resolved();
    assert!(issue.tracker);
    assert_eq!(issue.priority(), Some(2));
    assert_eq!(issue.assignee.as_deref(), Some("protostar"));
    assert_eq!(issue.assignee_id(), Some("agent"));
    assert_eq!(issue.assignee_kind(), Some("agent"));
    assert_eq!(issue.assignee_rank(), Some("captain"));
    let owner = issue.owner().unwrap();
    assert_eq!(
        (owner.id.as_str(), owner.name.as_str(), owner.kind.as_str()),
        ("owner", "Rosa Fixture", "human")
    );
    let progress = issue.progress().unwrap();
    assert_eq!((progress.done, progress.total), (3, 7));
    let prs = issue.pull_requests().unwrap();
    assert_eq!((prs.total, prs.open, prs.closed, prs.merged), (5, 1, 0, 4));
    assert_eq!(prs.checks.as_deref(), Some("success"));
    assert_eq!(issue.due_date(), Some("2026-11-20"));
    assert_eq!(issue.ready_for_agents(), Some(true));
    let activity = issue.latest_activity().unwrap();
    assert_eq!(
        (
            activity.kind.as_str(),
            activity.occurred_at.as_str(),
            activity.actor.as_deref()
        ),
        ("comment.created", "2026-11-16T12:00:00Z", Some("protostar"))
    );
}
#[test]
fn invalid_service_counts_are_bounded() {
    assert_eq!(super::count(-1), 0);
    assert_eq!(super::count(7), 7);
}
