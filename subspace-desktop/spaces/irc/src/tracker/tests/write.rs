//! What each tracker write sends, against the API client's offline mock,
//! and how a refusal reads.

use serde_json::{Value, json};
use subspace_api_client::Client;
use subspace_framework::client_test_support::Mock;

use super::super::plan::{Plan, Who};
use super::super::write::{self, Job};
use super::lyra;

/// A mock tracker: `me`, NAV-2 at version 4, and whatever a write returns.
fn tracker(ready: bool, assignee: Option<&str>) -> Mock {
    let issue = issue(ready, assignee);
    Mock::new(move |request| {
        let body = match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/api/v1/me") => {
                json!({"userId":"user-me","kind":"human","displayName":"Mira"})
            }
            ("GET", "/api/v1/issues/NAV-2") => issue.clone(),
            ("PATCH", _) => {
                let sent: Value = serde_json::from_str(&request.body).unwrap();
                let mut after = issue.clone();
                if let Some(id) = sent.get("assigneeId") {
                    after["assignee"] = json!({"userId":id,"displayName":"aurora-lyra","isFormerMember":false,"kind":"agent","agent":null});
                    after["readyForAgents"] = json!(false);
                }
                if let Some(flag) = sent.get("readyForAgents") {
                    after["readyForAgents"] = flag.clone();
                }
                after["version"] = json!(5);
                after
            }
            _ => return (404, json!({"detail":"Not found."}).to_string()),
        };
        (200, body.to_string())
    })
}

fn issue(ready: bool, assignee: Option<&str>) -> Value {
    crate::fixtures::tracker_issue(ready, assignee)
}

fn me() -> String {
    json!({"userId":"user-me","kind":"human","displayName":"Mira"}).to_string()
}

fn client(mock: &Mock) -> Client {
    Client::loopback(&format!("{}/api", mock.base_url()), None)
        .unwrap()
        .without_live()
}

fn sent(mock: &Mock) -> Vec<(String, String, Value)> {
    mock.requests()
        .into_iter()
        .map(|request| {
            let body = serde_json::from_str(&request.body).unwrap_or(Value::Null);
            (request.method, request.path, body)
        })
        .collect()
}

#[test]
fn assign_reads_the_issue_then_patches_only_the_assignee_at_its_version() {
    let mock = tracker(true, None);
    let job = Job::Assign {
        issue: "NAV-2".into(),
        to: Who::Agent(lyra()),
    };
    let done = write::run(&client(&mock), &job).unwrap();
    assert_eq!(
        done.notice,
        "Assigned NAV-2 to aurora-lyra. It is no longer flagged Ready for agents."
    );
    assert_eq!(done.changed, "NAV-2");
    let requests = sent(&mock);
    assert_eq!(requests.len(), 3, "me, the issue, one write");
    assert_eq!(requests[2].0, "PATCH");
    assert_eq!(requests[2].1, "/api/v1/issues/issue-2");
    assert_eq!(requests[2].2, json!({"assigneeId":"user-lyra","version":4}));
}

#[test]
fn ready_sends_the_flag_and_a_fresh_refusal_writes_nothing() {
    let mock = tracker(false, None);
    let on = Job::Ready {
        issue: "NAV-2".into(),
        on: true,
    };
    assert_eq!(
        write::run(&client(&mock), &on).unwrap().notice,
        "Flagged NAV-2 Ready for agents."
    );
    assert_eq!(sent(&mock)[2].2, json!({"readyForAgents":true,"version":4}));
    let assigned = tracker(false, Some("user-wren"));
    let refused = write::run(&client(&assigned), &on).unwrap_err().words;
    assert!(refused.contains("assigned to Wren"), "{refused}");
    assert!(sent(&assigned).iter().all(|(method, _, _)| method == "GET"));
}

#[test]
fn a_decision_posts_once_on_the_issue_s_project() {
    let mock = Mock::new(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/api/v1/me") => (
                200,
                json!({"userId":"user-me","kind":"human","displayName":"Mira"}).to_string(),
            ),
            ("GET", "/api/v1/issues/NAV-2") => (200, issue(false, None).to_string()),
            ("POST", "/api/v1/decisions") => (201, decision().to_string()),
            _ => (404, "{}".into()),
        },
    );
    let job = Job::from_plan(
        Plan::Decision {
            issue: "NAV-2".into(),
            text: "Decode tiles on a worker pool.\nNot on the UI thread.".into(),
        },
        "Recorded in Channels from #aurora-lyra on Acme IRC.".into(),
    );
    let done = write::run(&client(&mock), &job).unwrap();
    assert_eq!(
        done.notice,
        "Recorded a decision on NAV-2: “Decode tiles on a worker pool.”."
    );
    let requests = sent(&mock);
    let posted = &requests[2].2;
    assert_eq!(posted["projectId"], "project-nav");
    assert_eq!(posted["issueIds"], json!(["issue-2"]));
    assert_eq!(posted["title"], "Decode tiles on a worker pool.");
    assert_eq!(
        posted["context"],
        "Recorded in Channels from #aurora-lyra on Acme IRC."
    );
    assert!(
        posted.get("madeByType").is_none(),
        "the server records the signed-in person"
    );
}

fn decision() -> Value {
    serde_json::from_str(r#"{"id":"00000000-0000-0000-0000-000000000009","title":"Decode tiles on a worker pool.",
        "status":"proposed","decidedOn":"2026-11-22","projectId":"project-nav","projectKey":"NAV",
        "project":{"id":"project-nav","name":"Navigation","archived":false},
        "supersedesId":null,"supersededById":null,"supersedes":null,"supersededBy":null,
        "issues":{"items":[],"count":0},"tags":[],"madeByType":"human","madeByUserId":"user-me",
        "madeByAgentName":null,"madeByAgentRank":null,"madeByAgentModel":null,
        "evidenceKind":null,"evidenceReference":null,"evidenceExcerpt":null,
        "recordedByUserId":"user-me","version":1,"revisionNumber":1,
        "createdAt":"2026-11-22T12:00:00Z","updatedAt":"2026-11-22T12:00:00Z",
        "decision":"Decode tiles on a worker pool.","context":"","why":"","implications":"","alternatives":"",
        "acceptedAt":null,"rejectedAt":null,"acceptedByUserId":null,"rejectedByUserId":null,
        "provenance":{"madeByType":"human","madeByUserId":"user-me","madeByAgentName":null,
            "madeByAgentRank":null,"madeByAgentModel":null,"evidenceKind":null,"evidenceReference":null,"evidenceExcerpt":null},
        "recordedBy":{"userId":"user-me","displayName":"Mira","available":true,"kind":"human"}}"#).unwrap()
}

#[test]
fn the_api_s_refusal_reads_in_plain_words_without_its_body() {
    let mock = Mock::with_headers(|request| {
        let problem = vec![("Content-Type".into(), "application/problem+json".into())];
        match request.method.as_str() {
            "GET" if request.path == "/api/v1/me" => (200, Vec::new(), me()),
            "GET" => (200, Vec::new(), issue(false, None).to_string()),
            _ => (
                403,
                problem,
                json!({"detail":"secret server words"}).to_string(),
            ),
        }
    });
    let job = Job::Assign {
        issue: "NAV-2".into(),
        to: Who::Me,
    };
    let refused = write::run(&client(&mock), &job).unwrap_err().words;
    assert_eq!(
        refused,
        "Couldn't change NAV-2: You don't have permission to save this issue. Your draft is still here."
    );
    let missing = Mock::new(|request| match request.path.as_str() {
        "/api/v1/me" => (
            200,
            json!({"userId":"user-me","kind":"human","displayName":"Mira"}).to_string(),
        ),
        _ => (404, "{}".into()),
    });
    assert_eq!(
        write::run(&client(&missing), &job).unwrap_err().words,
        "NAV-2 isn't in the tracker, or you can't see it."
    );
}

#[test]
fn a_decision_the_tracker_never_confirmed_is_marked_unconfirmed() {
    let mock = Mock::new(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/api/v1/me") => (200, me()),
            ("GET", "/api/v1/issues/NAV-2") => (200, issue(false, None).to_string()),
            ("POST", "/api/v1/decisions") => (502, "{}".into()),
            _ => (404, "{}".into()),
        },
    );
    let job = Job::from_plan(
        Plan::Decision {
            issue: "NAV-2".into(),
            text: "Decode tiles on a worker pool.".into(),
        },
        String::new(),
    );
    let failed = write::run(&client(&mock), &job).unwrap_err();
    assert!(failed.unconfirmed, "{}", failed.words);
    assert!(failed.words.contains("Check the issue's decisions"));
}
