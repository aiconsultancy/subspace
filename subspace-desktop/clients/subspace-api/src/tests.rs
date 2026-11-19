use crate::{
    Client,
    hub::{Hub, HubSnapshot, HubStatus},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use subspace_framework::client_test_support::{Mock, Request};
mod activity;
mod chat_uploads;
mod comments;
#[cfg(debug_assertions)]
mod configuration;
pub(crate) mod decisions;
mod hub;
mod initiatives;
mod issue_moves;
mod issue_writes;
mod live;
mod media;
mod paging;
mod privacy;
pub(crate) mod pull_requests;
mod recovery;
mod summary;
mod timeline;
mod transport;
mod uploads;

fn client(mock: &Mock) -> Client {
    Client::loopback(&format!("{}/api", mock.base_url()), None)
        .unwrap()
        .without_live()
}

pub(crate) fn row(id: &str) -> Value {
    json!({"id":id,"identifier":format!("SUB-{id}"),"projectId":"project-1","projectKey":"SUB",
        "type":"issue","title":"A fixture issue","summary":"Keep issue context available so the team can understand the work before reading the full discussion.","state":{"id":"state-1","name":"Started","type":"started"},
        "assignee":null,"labels":[],"priority":2,"estimate":null,"dueDate":null,"milestoneId":null,
        "parentId":null,"blocked":false,"archivedAt":null,"createdAt":"2026-11-01T12:00:00Z",
        "updatedAt":"2026-11-01T12:00:00Z","version":1,"progress":null,"commentCount":0,
        "todoCounts":{"total":3,"done":1},"lastActivityAt":"2026-11-16T12:00:00Z","lastActivity":null,
        "cycle":null,"relayedCrew":null,"provenance":null})
}

pub(crate) fn detail(id: &str) -> Value {
    let mut issue = row(id);
    issue.as_object_mut().unwrap().extend(
        json!({"description":"Fixture description","todos":[],
        "createdBy":person("me"),"startedAt":null,"completedAt":null,"canceledAt":null,
        "archivedBy":null,"parent":null,"duplicateOf":null,
        "children":{"items":[],"nextCursor":null},"relations":{"items":[],"nextCursor":null}})
        .as_object()
        .unwrap()
        .clone(),
    );
    issue
}

fn person(id: &str) -> Value {
    json!({"userId":id,"kind":"human","displayName":"Ada Fixture","isFormerMember":false})
}

fn comment(id: &str, body: &str, parent: Option<&str>) -> Value {
    json!({"id":id,"issueId":"1","parentCommentId":parent,"body":body,"version":1,
        "actualAuthor":person("me"),"createdAt":chrono::Utc::now().to_rfc3339(),
        "displayCreatedAt":chrono::Utc::now().to_rfc3339(),"editedAt":null,"deletedAt":null,
        "resolved":false,"resolvedBy":null,"resolvedAt":null,"replyCount":0,"hasReplies":false,
        "media":[],"relayedCrew":null,"provenance":null})
}

pub(crate) fn page(items: Vec<Value>, cursor: Option<&str>) -> String {
    json!({"items":items,"nextCursor":cursor}).to_string()
}

pub(crate) fn common(request: &Request) -> (u16, String) {
    common_path(&request.path)
}

pub(crate) fn common_path(path: &str) -> (u16, String) {
    let path = path.split('?').next().unwrap();
    match path {
        "/api/v1/me" => (
            200,
            json!({"userId":"me","kind":"human","displayName":"Ada Fixture"}).to_string(),
        ),
        "/api/v1/projects" => (
            200,
            page(
                vec![
                    json!({"id":"project-1","key":"SUB","name":"Fixture project","defaultStateId":"state-1","archivedAt":null}),
                ],
                None,
            ),
        ),
        "/api/v1/projects/SUB/workflow-states" => (
            200,
            page(
                vec![
                    json!({"id":"state-1","projectId":"project-1","name":"Started","type":"started","position":1,"isDefault":true}),
                ],
                None,
            ),
        ),
        "/api/v1/issues/groups" => (200, json!({"groupBy":"stateType","groups":[]}).to_string()),
        "/api/v1/issues/SUB-1" | "/api/v1/issues/1" => (200, detail("1").to_string()),
        "/api/v1/issues/1/comments" | "/api/v1/issues/SUB-1/comments" => (200, page(vec![], None)),
        "/api/v1/issues/SUB-1/activity" | "/api/v1/issues/1/activity" => (
            200,
            page(
                vec![json!({
                    "id":"root-activity","kind":"comment.created","occurredAt":"2026-11-16T12:00:00Z","displayAt":"2026-11-16T12:00:00Z",
                    "actor":{"type":"system","userId":null,"person":null,"label":"Fixture service"},
                    "comment":comment("root-1", "Fixture root", None)
                })],
                None,
            ),
        ),
        _ => (404, "{}".into()),
    }
}

pub(crate) fn wait(hub: &Hub, predicate: impl Fn(&HubSnapshot) -> bool) -> HubSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = hub.snapshot();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "hub state did not reach expected outcome: {:?}",
            snapshot.status()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn connected(mock: &Mock) -> Hub {
    let hub = Hub::from_client(client(mock), "Subspace API");
    wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    hub.open_issue("SUB-1", crate::PROFILE_ID).unwrap();
    wait(&hub, |snapshot| {
        snapshot
            .issue()
            .is_some_and(|issue| issue.detail().is_some())
    });
    hub
}
