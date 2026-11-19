//! Fictional Subspace API records for tests, built from JSON because slice 1's DTOs
//! are `#[non_exhaustive]`. People and titles follow the design's fixtures.

use serde_json::{Value, json};
use subspace_api_client::model::{IssueListItem, Project};

pub(crate) fn project(key: &str, name: &str, archived: bool) -> Project {
    serde_json::from_value(json!({
        "id": format!("project-{key}"),
        "key": key,
        "name": name,
        "defaultStateId": "state-todo",
        "archivedAt": archived.then_some("2026-10-01T00:00:00Z"),
        "states": [],
    }))
    .unwrap()
}

/// An issue in state `state` of type `kind`; `extra` overrides any field.
pub(crate) fn issue(
    identifier: &str,
    title: &str,
    state: (&str, &str),
    extra: Value,
) -> IssueListItem {
    let mut value = json!({
        "id": format!("id-{identifier}"),
        "identifier": identifier,
        "projectId": "project-SUB",
        "projectKey": "SUB",
        "type": "issue",
        "title": title,
        "summary": "Keep the work visible so the team can understand its purpose before opening the description.",
        "state": {"id": format!("state-{}", state.1), "name": state.0, "type": state.1},
        "assignee": null,
        "priority": 0,
        "labels": [],
        "blocked": false,
        "createdAt": "2026-11-01T09:00:00Z",
        "updatedAt": "2026-11-14T09:00:00Z",
        "version": 1,
        "commentCount": 0,
        "todoCounts": {"total": 0, "done": 0},
        "lastActivityAt": "2026-11-14T09:00:00Z",
    });
    if let (Some(base), Value::Object(extra)) = (value.as_object_mut(), extra) {
        base.extend(extra);
    }
    serde_json::from_value(value).unwrap()
}

pub(crate) fn person(name: &str) -> Value {
    json!({"userId": format!("user-{name}"), "displayName": name, "isFormerMember": false, "kind": "human"})
}
