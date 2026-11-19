use super::{comment, common, connected, detail, page, person};
use crate::{
    IssueQuery, ListQuery,
    model::{Comment, IssueDetail, IssueGroups, IssueListItem, IssueReference, Page, Todo},
};
use serde_json::{Value, json};
use subspace_framework::client_test_support::Mock;

const TEXT: &str = "private-provider-text";

fn private_issue() -> Value {
    let mut issue = detail("1");
    issue["title"] = json!(TEXT);
    issue["description"] = json!(TEXT);
    issue["state"]["name"] = json!(TEXT);
    let mut author = person("me");
    author["displayName"] = json!(TEXT);
    issue["assignee"] = author.clone();
    issue["labels"] = json!([{"id":"label-1","name":TEXT,"color":"#ffffff","state":"active"}]);
    issue["todos"] = json!([{"id":"todo-1","issueId":"1","text":TEXT,"done":false,"position":0,
        "createdBy":author,"createdAt":"2026-11-16T12:00:00Z","doneBy":null,"doneAt":null,
        "updatedBy":null,"updatedAt":"2026-11-16T12:00:00Z","version":1,"relayedCrew":null,"updatedRelayedCrew":null}]);
    issue
}

fn assert_redacted(value: &dyn std::fmt::Debug) {
    for output in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(
            !output.contains(TEXT),
            "provider text leaked in diagnostics"
        );
        assert!(output.contains("redacted") || output.contains("count"));
    }
}

#[test]
fn issue_comment_todo_and_reference_diagnostics_redact_provider_text() {
    let detail: IssueDetail = serde_json::from_value(private_issue()).unwrap();
    let mut post = comment("comment-1", TEXT, None);
    post["actualAuthor"]["displayName"] = json!(TEXT);
    let post: Comment = serde_json::from_value(post).unwrap();
    let result = crate::CommentPost::Created(Box::new(post.clone()));
    let reference: IssueReference =
        serde_json::from_value(json!({"id":"1","identifier":"SUB-1","title":TEXT,
        "type":"issue","state":{"id":"state-1","name":TEXT,"type":"started"},"archived":false}))
        .unwrap();
    let todo: &Todo = &detail.todos[0];
    for value in [
        &detail as &dyn std::fmt::Debug,
        &detail.fields,
        &detail.fields.state,
        &detail.fields.assignee,
        &detail.fields.labels,
        &post,
        &result,
        todo,
        &reference,
    ] {
        assert_redacted(value);
    }
    let output = format!("{detail:?} {post:?} {todo:?} {reference:?}");
    for id in ["SUB-1", "comment-1", "todo-1", "reply_count"] {
        assert!(output.contains(id));
    }
}

#[test]
fn pages_groups_queries_and_populated_hub_snapshots_do_not_leak_text() {
    let item: IssueListItem = serde_json::from_value(private_issue()).unwrap();
    let page: Page<IssueListItem> =
        serde_json::from_str(&page(vec![private_issue()], Some(TEXT))).unwrap();
    let groups: IssueGroups = serde_json::from_value(json!({"groupBy":"stateType","groups":[
        {"stateType":"started","count":1,"items":[private_issue()],"nextCursor":TEXT}
    ]}))
    .unwrap();
    let mock = Mock::new(|request| {
        if request.path == "/api/v1/issues/SUB-1" {
            (200, private_issue().to_string())
        } else {
            common(request)
        }
    });
    let hub = connected(&mock);
    let snapshot = hub.snapshot();
    let query = ListQuery::default().search(TEXT);
    let mut issue_query = IssueQuery::new("SUB");
    issue_query.search = Some(TEXT.into());
    for value in [
        &item as &dyn std::fmt::Debug,
        &page,
        &groups,
        &groups.groups[0],
        &snapshot,
        snapshot.issue().unwrap(),
        &query,
        &issue_query,
    ] {
        assert_redacted(value);
    }
    assert!(format!("{snapshot:?}").contains("SUB-1"));
    assert!(format!("{page:?}").contains("item_count: 1"));
    hub.stop();
}
