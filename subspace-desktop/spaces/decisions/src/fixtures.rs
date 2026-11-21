//! Fictional decisions, people and projects for tests, built from JSON
//! because the client's records are `#[non_exhaustive]`. The names follow
//! the approved mock-up's fixtures; none is a real person or service.

use std::sync::Arc;

use serde_json::{Value, json};
use subspace_api_client::model::{DecisionDetail, DecisionListItem, Me, Project};
use subspace_api_client::{DecisionQuery, Error};

use crate::load::{Answer, Directory, Found, Plan, Reader};
use crate::words::People;

/// The signed-in person, who recorded every fixture decision.
pub(crate) const OPERATOR: &str = "user-operator";
pub(crate) const OPERATOR_NAME: &str = "local-operator";

/// A related decision: its id, title and status.
type Related = (&'static str, &'static str, &'static str);

const RELAY_AFTER: Related = (
    "d2",
    "Read the relay position after the correction is applied",
    "accepted",
);
const RELAY_BEFORE: Related = (
    "d6",
    "Read the relay position before each burn",
    "superseded",
);
const STREAM: Related = ("d4", "Stream warp telemetry every five seconds", "accepted");
const POLL: Related = (
    "d7",
    "Poll warp telemetry every thirty seconds",
    "superseded",
);
const PREVIEW: (&str, &str) = ("NAV-2", "Course correction preview");
const CALIBRATE: (&str, &str) = ("NAV-5", "Calibrate the star tracker fixtures");
const EXPORT: (&str, &str) = ("WARP-7", "Warp field telemetry export");

pub(crate) fn project(key: &str, name: &str) -> Project {
    serde_json::from_value(json!({
        "id": format!("project-{key}"),
        "key": key,
        "name": name,
        "defaultStateId": "state-todo",
        "archivedAt": null,
    }))
    .unwrap()
}

pub(crate) fn projects() -> Vec<Project> {
    vec![project("NAV", "Navigation"), project("WARP", "Warp drive")]
}

/// The account's people, as the people read names them.
pub(crate) fn people() -> People {
    [
        (OPERATOR, OPERATOR_NAME),
        ("agent-lyra", "aurora-lyra (agent)"),
    ]
    .into_iter()
    .map(|(id, name)| (id.to_owned(), name.to_owned()))
    .collect()
}

fn project_name(key: &str) -> &'static str {
    if key == "WARP" {
        "Warp drive"
    } else {
        "Navigation"
    }
}

/// A decision made and recorded by the operator, with no links.
pub(crate) fn item_value(
    id: &str,
    project: &str,
    title: &str,
    status: &str,
    decided: &str,
) -> Value {
    json!({
        "id": id, "title": title, "status": status, "decidedOn": decided,
        "projectId": format!("project-{project}"), "projectKey": project,
        "project": {"id": format!("project-{project}"), "name": project_name(project), "archived": false},
        "issues": {"items": [], "count": 0},
        "tags": [project.to_lowercase()],
        "madeByType": "human", "madeByUserId": OPERATOR,
        "recordedByUserId": OPERATOR, "revisionNumber": 1,
        "createdAt": "2026-11-18T10:00:00Z", "updatedAt": "2026-11-18T10:00:00Z",
    })
}

fn agent(value: &mut Value, name: &str, rank: &str) {
    value["madeByType"] = json!("agent");
    value["madeByUserId"] = Value::Null;
    value["madeByAgentName"] = json!(name);
    value["madeByAgentRank"] = json!(rank);
    value["madeByAgentModel"] = json!("fixture-model");
}

fn issues(value: &mut Value, links: &[(&str, &str)]) {
    let items: Vec<Value> = links
        .iter()
        .map(|(identifier, title)| {
            json!({"id": format!("issue-{identifier}"), "identifier": identifier, "title": title})
        })
        .collect();
    value["issues"] = json!({"items": items, "count": links.len()});
}

/// `field` is `supersedes` or `supersededBy`.
fn relation(value: &mut Value, field: &str, (id, title, status): Related) {
    value[format!("{field}Id")] = json!(id);
    value[field] = json!({"id": id, "title": title, "status": status});
}

fn evidence(value: &mut Value, reference: &str, excerpt: &str) {
    value["evidenceKind"] = json!("ircMessage");
    value["evidenceReference"] = json!(reference);
    value["evidenceExcerpt"] = json!(excerpt);
}

fn proposed() -> Value {
    let mut value = item_value(
        "d1",
        "WARP",
        "Retry the plasma injector handshake three times",
        "proposed",
        "2026-11-20",
    );
    agent(&mut value, "borealis-vega", "captain");
    issues(
        &mut value,
        &[("WARP-8", "Retry the plasma injector handshake")],
    );
    evidence(
        &mut value,
        "#borealis-warp",
        "Backoff schedule agreed: 1, 2 and 4 seconds.",
    );
    value
}

fn accepted() -> [Value; 4] {
    let mut relay = item_value("d2", "NAV", RELAY_AFTER.1, "accepted", "2026-11-19");
    agent(&mut relay, "aurora-lyra", "captain");
    issues(&mut relay, &[PREVIEW, CALIBRATE]);
    relation(&mut relay, "supersedes", RELAY_BEFORE);
    evidence(
        &mut relay,
        "https://example.invalid/aurora-nav",
        "The preview reads the relay position after the correction.",
    );
    let mut headings = item_value(
        "d3",
        "NAV",
        "Keep the last ten headings in the console",
        "accepted",
        "2026-11-18",
    );
    issues(
        &mut headings,
        &[("NAV-4", "Show heading history in the console")],
    );
    let mut stream = item_value("d4", "WARP", STREAM.1, "accepted", "2026-11-16");
    agent(&mut stream, "aurora-admiral", "rearAdmiral");
    issues(&mut stream, &[EXPORT]);
    relation(&mut stream, "supersedes", POLL);
    stream["tags"] = json!(["warp", "telemetry"]);
    let beside = item_value(
        "d5",
        "NAV",
        "Fixtures live beside the code they test",
        "accepted",
        "2026-11-12",
    );
    [relay, headings, stream, beside]
}

fn rejected() -> Value {
    let mut value = item_value(
        "d8",
        "NAV",
        "Skip calibration when the fixture is under a day old",
        "rejected",
        "2026-11-10",
    );
    agent(&mut value, "aurora-orion", "captain");
    issues(&mut value, &[CALIBRATE]);
    value
}

fn superseded() -> [Value; 2] {
    let mut relay = item_value("d6", "NAV", RELAY_BEFORE.1, "superseded", "2026-11-04");
    agent(&mut relay, "aurora-orion", "captain");
    issues(&mut relay, &[PREVIEW]);
    relation(&mut relay, "supersededBy", RELAY_AFTER);
    let mut poll = item_value("d7", "WARP", POLL.1, "superseded", "2026-11-01");
    agent(&mut poll, "aurora-admiral", "rearAdmiral");
    issues(&mut poll, &[EXPORT]);
    relation(&mut poll, "supersededBy", STREAM);
    poll["tags"] = json!(["warp", "telemetry"]);
    [relay, poll]
}

/// The eight fixture decisions as the API lists them, newest decided first.
pub(crate) fn item_values() -> Vec<Value> {
    let [relay, headings, stream, beside] = accepted();
    let [relay_before, poll] = superseded();
    vec![
        proposed(),
        relay,
        headings,
        stream,
        beside,
        rejected(),
        relay_before,
        poll,
    ]
}

pub(crate) fn items() -> Vec<DecisionListItem> {
    item_values()
        .into_iter()
        .map(|value| serde_json::from_value(value).unwrap())
        .collect()
}

pub(crate) fn item(id: &str) -> DecisionListItem {
    items().into_iter().find(|item| item.id == id).unwrap()
}

/// The detail of fixture decision `id`; its Alternatives are empty.
pub(crate) fn detail(id: &str) -> Option<DecisionDetail> {
    let mut value = item_values().into_iter().find(|value| value["id"] == id)?;
    let accepted = value["status"] == "accepted";
    let fields = json!({
        "decision": "The preview reads the relay position **after** the correction is applied.",
        "context": "The preview drifted when the correction landed late.",
        "why": "Reading after the correction matches what the helm sees.",
        "implications": "Older previews show a different position.",
        "alternatives": "",
        "acceptedAt": accepted.then_some("2026-11-19T09:30:00Z"),
        "acceptedByUserId": accepted.then_some(OPERATOR),
        "provenance": {"madeByType": value["madeByType"].clone()},
        "recordedBy": {"userId": OPERATOR, "displayName": OPERATOR_NAME, "available": true, "kind": "human"},
    });
    value.as_object_mut()?.extend(fields.as_object()?.clone());
    serde_json::from_value(value).ok()
}

fn directory() -> Directory {
    let me: Me = serde_json::from_value(json!({
        "userId": OPERATOR,
        "kind": "human",
        "displayName": OPERATOR_NAME,
    }))
    .unwrap();
    Directory {
        me,
        projects: projects(),
        people: people(),
    }
}

/// Whether the API would list `item` for `query`.
fn keeps(item: &DecisionListItem, query: &DecisionQuery) -> bool {
    let search = query.q.as_deref().map(str::to_lowercase);
    (query.status.is_empty() || query.status.contains(&item.status))
        && query
            .made_by_type
            .is_none_or(|kind| kind == item.made_by_type)
        && query
            .project
            .as_deref()
            .is_none_or(|key| key == item.project_key)
        && query
            .tag
            .as_deref()
            .is_none_or(|tag| item.tags.iter().any(|t| t == tag))
        && search.is_none_or(|text| item.title.to_lowercase().contains(&text))
}

fn answer(plan: &Plan) -> Answer {
    let items = items()
        .into_iter()
        .filter(|item| keeps(item, &plan.query))
        .collect();
    Answer {
        directory: plan.directory.then(|| Ok(directory())),
        found: Ok(Found { items, more: false }),
    }
}

/// Reads that answer from the fixture decisions at once.
pub(crate) fn reader() -> Reader {
    Reader {
        list: Arc::new(answer),
        detail: Arc::new(|id: &str| detail(id).ok_or(Error::NotFound)),
    }
}

/// Reads that fail with `error`.
pub(crate) fn failing(error: Error) -> Reader {
    Reader {
        list: Arc::new(move |plan: &Plan| Answer {
            directory: plan.directory.then_some(Err(error)),
            found: Err(error),
        }),
        detail: Arc::new(move |_: &str| Err(error)),
    }
}
