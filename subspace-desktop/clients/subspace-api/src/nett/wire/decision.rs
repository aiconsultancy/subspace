//! A NETT decision as the desktop's decision row, with its detail when NETT
//! sends the decision's text.

use serde_json::{Value, json};

/// A NETT decision as the desktop's decision row. NETT records human
/// deciders, a planning project and an initiative; it has no tags, agent
/// makers or evidence, which stay empty.
pub(crate) fn decision(item: &Value) -> Value {
    let decider_id = item
        .get("deciders")
        .and_then(Value::as_array)
        .and_then(|deciders| deciders.first())
        .and_then(|decider| decider.get("userId"))
        .and_then(Value::as_str);
    let recorded_by = item
        .get("createdByUserId")
        .and_then(Value::as_str)
        .or(decider_id)
        .unwrap_or_default();
    let mut row = row(item, decider_id, recorded_by);
    if item.get("decision").is_some()
        && let (Some(row), Value::Object(detail)) =
            (row.as_object_mut(), detail(item, decider_id, recorded_by))
    {
        row.extend(detail);
    }
    row
}

fn text<'a>(item: &'a Value, key: &str) -> &'a str {
    item.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The fields every decision row has.
fn row(item: &Value, decider_id: Option<&str>, recorded_by: &str) -> Value {
    let summary_id = |key: &str| {
        item.get(key)
            .and_then(|summary| summary.get("id"))
            .cloned()
            .unwrap_or(Value::Null)
    };
    json!({
        "id": text(item, "id"),
        "title": text(item, "title"),
        "status": text(item, "status"),
        "decidedOn": text(item, "decidedOn"),
        "projectId": text(item, "projectId"),
        "projectKey": "",
        "project": item.get("project").cloned().unwrap_or(Value::Null),
        "supersedesId": summary_id("supersedes"),
        "supersededById": summary_id("supersededBy"),
        "supersedes": item.get("supersedes").cloned().unwrap_or(Value::Null),
        "supersededBy": item.get("supersededBy").cloned().unwrap_or(Value::Null),
        "issues": issues(item),
        "tags": [],
        "madeByType": "human",
        "madeByUserId": decider_id,
        "recordedByUserId": recorded_by,
        "version": item.get("version").cloned().unwrap_or(Value::Null),
        "revisionNumber": item.get("revisionNumber").cloned().unwrap_or(json!(1)),
        "createdAt": text(item, "createdAt"),
        "updatedAt": text(item, "updatedAt"),
    })
}

/// The linked issues NETT sends, with its count of them all.
fn issues(item: &Value) -> Value {
    let issues = item.get("issues").cloned().unwrap_or_default();
    let items: Vec<Value> = issues
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|issue| {
            json!({
                "id": text(issue, "id"),
                "identifier": text(issue, "identifier"),
                "title": text(issue, "title"),
            })
        })
        .collect();
    let count = issues
        .get("count")
        .and_then(Value::as_i64)
        .unwrap_or(items.len() as i64);
    json!({"items": items, "count": count})
}

/// The decision's text and who recorded it, which may not be who made it:
/// named from the deciders when the recorder is one of them.
fn detail(item: &Value, decider_id: Option<&str>, recorded_by: &str) -> Value {
    let recorder = item
        .get("deciders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|person| person.get("userId").and_then(Value::as_str) == Some(recorded_by))
        .cloned()
        .unwrap_or_default();
    json!({
        "decision": text(item, "decision"),
        "context": text(item, "context"),
        "why": text(item, "why"),
        "implications": text(item, "implications"),
        "alternatives": "",
        "acceptedAt": item.get("acceptedAt").cloned().unwrap_or(Value::Null),
        "acceptedByUserId": item.get("acceptedByUserId").cloned().unwrap_or(Value::Null),
        "provenance": {"madeByType": "human", "madeByUserId": decider_id},
        "recordedBy": {
            "userId": recorded_by,
            "displayName": recorder
                .get("displayName")
                .or_else(|| item.get("createdByName"))
                .and_then(Value::as_str)
                .unwrap_or_default(),
            "available": recorder.get("available").and_then(Value::as_bool).unwrap_or(true),
            "kind": "human",
        },
    })
}
