//! Mapping raw agent-task documents and their pull request artifacts.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use super::browser::task_browser_url;
use super::{PullRequestArtifact, RawArtifact, Subject, TASK_WINDOW_DAYS, Task, pull_state};

pub(super) fn to_task(
    repo: &str,
    raw: &Value,
    subject: Option<Subject>,
    pulls: &HashMap<u64, PullRequestArtifact>,
) -> Task {
    let a = raw.as_object();
    let get = |k| a.and_then(|m| m.get(k));
    Task {
        id: string(get("id")),
        repo: repo.to_owned(),
        name: string(get("name")),
        state: fallback(string(get("state")), "queued"),
        html_url: task_browser_url(raw, task_pull(raw, pulls)),
        created_at: string(get("created_at")),
        updated_at: string(get("updated_at")),
        pull_request: task_pull(raw, pulls).cloned(),
        subject,
    }
}
fn string(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}
fn fallback(value: String, default: &str) -> String {
    if value.is_empty() {
        default.to_owned()
    } else {
        value
    }
}
pub(super) fn raw_artifact(raw: &Value) -> Option<RawArtifact> {
    raw.get("artifacts")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .find(|a: &RawArtifact| {
            a.kind.as_deref() == Some("pull") && a.data.as_ref().and_then(|d| d.id).is_some()
        })
}
pub(super) fn since_30_days() -> String {
    // A `Z` suffix needs no URL encoding; a raw `+00:00` reads as a space.
    (chrono::Utc::now() - chrono::Duration::days(TASK_WINDOW_DAYS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Paths that follow the host but never name a repository owner.
const RESERVED_SEGMENTS: &[&str] = &[
    "copilot",
    "orgs",
    "settings",
    "users",
    "apps",
    "notifications",
    "codespaces",
    "enterprises",
    "marketplace",
    "sponsors",
];

fn repo_segment(part: Option<&str>) -> Option<&str> {
    let part = part.filter(|p| !p.is_empty())?;
    part.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        .then_some(part)
}

/// The `owner/name` a web URL points at, when its first two path segments
/// name a repository. `GET /agents/tasks` reports `repository.id` only, so
/// the row's own links carry the name instead.
pub(super) fn repo_from_url(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let mut parts = rest.split('/').skip(1);
    let owner = repo_segment(parts.next())?;
    let name = repo_segment(parts.next())?;
    (!RESERVED_SEGMENTS.contains(&owner)).then(|| format!("{owner}/{name}"))
}

/// The repository of a task the list endpoint did not name: its own page
/// first, then the pull request its artifact resolved to.
pub(super) fn task_repo(raw: &Value, pull: Option<&PullRequestArtifact>) -> String {
    raw.get("repository")
        .and_then(|repo| repo.get("full_name"))
        .and_then(Value::as_str)
        .filter(|repo| crate::validate::check_repo(repo).is_ok())
        .map(str::to_owned)
        .or_else(|| {
            raw.get("html_url")
                .and_then(Value::as_str)
                .and_then(repo_from_url)
        })
        .or_else(|| repo_from_url(&pull?.url))
        .unwrap_or_default()
}

/// The pull request `raw`'s artifact resolved to, if any.
pub(super) fn task_pull<'a>(
    raw: &Value,
    pulls: &'a HashMap<u64, PullRequestArtifact>,
) -> Option<&'a PullRequestArtifact> {
    pulls.get(&raw_artifact(raw)?.data?.id?)
}

/// A task the response did not name a repository for: the name comes from
/// its own links.
pub(super) fn linked_task(
    raw: &Value,
    subject: Option<Subject>,
    pulls: &HashMap<u64, PullRequestArtifact>,
) -> Task {
    to_task(&task_repo(raw, task_pull(raw, pulls)), raw, subject, pulls)
}

#[derive(Deserialize)]
pub(super) struct PullNodes {
    pub(super) nodes: Vec<Option<PullNode>>,
}

#[derive(Deserialize)]
pub(super) struct PullNode {
    #[serde(rename = "databaseId")]
    database_id: Option<u64>,
    number: Option<u32>,
    url: Option<String>,
    state: Option<String>,
    #[serde(rename = "isDraft")]
    is_draft: Option<bool>,
}

pub(super) const PULL_NODES_QUERY: &str = "query($ids: [ID!]!) { nodes(ids: $ids) { ... on PullRequest { \
                                databaseId number url state isDraft } } }";

pub(super) const PULLS_PER_PAGE: u32 = 100;

/// The pull requests of `nodes`, keyed by database id.
pub(super) fn artifacts_from_nodes(
    nodes: Vec<Option<PullNode>>,
) -> HashMap<u64, PullRequestArtifact> {
    let mut pulls = HashMap::new();
    for node in nodes.into_iter().flatten() {
        if let (Some(id), Some(number), Some(url)) = (node.database_id, node.number, node.url) {
            let state = pull_state(node.state.as_deref(), node.is_draft.unwrap_or(false), false);
            pulls.insert(id, PullRequestArtifact { number, url, state });
        }
    }
    pulls
}

/// True when an artifact names a pull request `pulls` does not hold yet.
pub(super) fn needs_rest_lookup(
    artifacts: &[RawArtifact],
    pulls: &HashMap<u64, PullRequestArtifact>,
) -> bool {
    artifacts.iter().any(|artifact| {
        artifact
            .data
            .as_ref()
            .and_then(|data| data.id)
            .is_some_and(|id| !pulls.contains_key(&id))
    })
}
