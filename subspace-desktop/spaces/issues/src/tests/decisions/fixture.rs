use super::{FIRST, NEW, NEXT, OLD, Offline, offline_with};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};

pub(super) fn item(id: &str, title: &str, status: &str) -> Value {
    json!({"id":id,"title":title,"status":status,"decidedOn":"2026-11-12",
        "projectId":"fixture","projectKey":"SUB","project":{"id":"fixture","name":"Subspace","archived":false},
        "issues":{"items":[{"id":"2","identifier":"SUB-2","title":"Linked fixture issue"}],"count":1},
        "tags":["fixture"],"madeByType":"agent","madeByAgentName":"fixture-agent","madeByAgentRank":"crew",
        "madeByAgentModel":"fixture-model","recordedByUserId":"person-1","revisionNumber":1,
        "createdAt":"2026-11-18T10:00:00Z","updatedAt":"2026-11-18T10:00:00Z"})
}

fn detail(id: &str) -> Value {
    let mut item = if id == NEW {
        item(id, "Read decisions here", "accepted")
    } else {
        item(id, "Older choice", "superseded")
    };
    if id == NEW {
        item["supersedesId"] = json!(OLD);
        item["supersedes"] = json!({"id":OLD,"title":"Older choice","status":"superseded"});
        item["supersededById"] = json!("unavailable");
        item["madeByType"] = json!("human");
        item["evidenceKind"] = json!("documentOrUrl");
        item["evidenceReference"] = json!("https://example.invalid/design");
    } else {
        item["madeByAgentName"] = Value::Null;
        item["madeByAgentRank"] = Value::Null;
        item["madeByAgentModel"] = Value::Null;
    }
    let fields = json!({"decision":"**Read** them within the issue.","context":"Context text","why":"Why text",
        "implications":"Implications text","alternatives":"", "provenance":{"madeByType":"agent"},
        "recordedBy":{"userId":"person-1","displayName":"Fixture recorder","available":false,"kind":"agent"}});
    item.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    item
}

fn timeline() -> Value {
    json!({"items":[{"id":"event-decision","kind":"decision.logged","occurredAt":"2026-11-16T12:00:00Z",
        "displayAt":"2026-11-16T12:00:00Z","actor":{"type":"system","label":"Fixture service"},"data":{"decisionId":NEW}}],
        "nextCursor":null,"references":{"decisions":[{"id":NEW,"title":"Read decisions here","status":"accepted"}]}})
}

pub(in crate::tests) fn normal_fixture() -> Offline {
    fixture(200, 200, 200)
}

pub(super) fn fixture(first: u16, next: u16, detail_code: u16) -> Offline {
    switched(
        Arc::new(AtomicU16::new(first)),
        Arc::new(AtomicU16::new(next)),
        Arc::new(AtomicU16::new(detail_code)),
    )
}

pub(super) fn switched(
    first: Arc<AtomicU16>,
    next: Arc<AtomicU16>,
    detail_code: Arc<AtomicU16>,
) -> Offline {
    offline_with(move |r| {
        let path = r.path.as_str();
        let (code, response) = match path {
            FIRST => (
                first.load(Ordering::Relaxed),
                json!({"items":[item(NEW, "Read decisions here", "accepted")],"nextCursor":"next"}),
            ),
            NEXT => (
                next.load(Ordering::Relaxed),
                json!({"items":[item(OLD, "Older choice", "superseded")],"nextCursor":null}),
            ),
            _ if path.starts_with("/api/v1/decisions/") => (
                detail_code.load(Ordering::Relaxed),
                detail(path.rsplit('/').next().unwrap()),
            ),
            _ if path.starts_with("/api/v1/issues/SUB-1/activity?") => (200, timeline()),
            _ => return None,
        };
        let (status, body) = match code {
            200 => (200, response),
            204 => (200, json!({"items":[],"nextCursor":null})),
            _ => (code, json!({})),
        };
        Some((status, body.to_string()))
    })
}

/// One decision whose reasoning and linked issues overflow the panel, so
/// its controls start below the viewport.
pub(super) fn long_fixture() -> Offline {
    let detail_path = format!("/api/v1/decisions/{NEW}");
    offline_with(move |r| {
        let path = r.path.as_str();
        let body = match path {
            FIRST => {
                json!({"items":[item(NEW, "Read decisions here", "accepted")],"nextCursor":null})
            }
            _ if path == detail_path => long_detail(),
            _ if path.starts_with("/api/v1/issues/SUB-1/activity?") => timeline(),
            _ => return None,
        };
        Some((200, body.to_string()))
    })
}

pub(super) const LONG_LINKS: usize = 24;

fn long_detail() -> Value {
    let mut value = detail(NEW);
    let context: Vec<String> = (1..=40)
        .map(|n| format!("Context paragraph {n}."))
        .collect();
    value["context"] = json!(context.join("\n\n"));
    let links: Vec<Value> = (1..=LONG_LINKS)
        .map(|n| json!({"id":format!("link-{n}"),"identifier":format!("LNK-{n}"),"title":format!("Linked issue {n}")}))
        .collect();
    value["issues"] = json!({"items":links,"count":LONG_LINKS});
    value
}
