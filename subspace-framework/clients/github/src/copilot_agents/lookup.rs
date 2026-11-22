//! Bounded fallbacks for task responses without repository names or pull node ids.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use super::map::{raw_artifact, task_pull, task_repo};
use super::{PullRequestArtifact, RawArtifact};
use crate::PinnedWork;

/// Every lookup goes out with the operation's pinned credential.
pub(super) fn resolve_task_pulls(
    work: &PinnedWork<'_>,
    tasks: &mut [Value],
) -> HashMap<u64, PullRequestArtifact> {
    let artifacts: Vec<RawArtifact> = tasks.iter().filter_map(raw_artifact).collect();
    let mut pulls = work.pulls_by_node_id(&artifacts);
    fill_task_repositories(work, tasks, &pulls);
    for repo in unresolved_repositories(tasks, &pulls) {
        work.fill_pulls_from_rest(&repo, &mut pulls);
    }
    pulls
}

fn fill_task_repositories(
    work: &PinnedWork<'_>,
    tasks: &mut [Value],
    pulls: &HashMap<u64, PullRequestArtifact>,
) {
    let names = missing_repository_ids(tasks, pulls)
        .into_iter()
        .filter_map(|id| Some((id, repository_name(work, id)?)))
        .collect();
    apply_repository_names(tasks, &names);
}

fn repository_name(work: &PinnedWork<'_>, id: u64) -> Option<String> {
    let url = format!("{}/repositories/{id}", work.client.api_url);
    let raw: Value = work.rest_json(&url).ok()?;
    let name = raw["full_name"].as_str()?;
    crate::validate::check_repo(name).ok()?;
    Some(name.to_owned())
}
fn missing_repository_ids(
    tasks: &[Value],
    pulls: &HashMap<u64, PullRequestArtifact>,
) -> BTreeSet<u64> {
    tasks
        .iter()
        .filter(|raw| task_repo(raw, task_pull(raw, pulls)).is_empty())
        .filter_map(|raw| raw["repository"]["id"].as_u64().filter(|id| *id > 0))
        .collect()
}

fn apply_repository_names(tasks: &mut [Value], names: &HashMap<u64, String>) {
    for raw in tasks {
        if let Some(name) = raw["repository"]["id"]
            .as_u64()
            .and_then(|id| names.get(&id))
        {
            raw["repository"]["full_name"] = Value::String(name.clone());
        }
    }
}

fn unresolved_repositories(
    tasks: &[Value],
    pulls: &HashMap<u64, PullRequestArtifact>,
) -> BTreeSet<String> {
    tasks
        .iter()
        .filter_map(|raw| {
            let artifact = raw_artifact(raw)?;
            let id = artifact.data?.id?;
            if pulls.contains_key(&id) {
                return None;
            }
            let repo = task_repo(raw, None);
            (!repo.is_empty()).then_some(repo)
        })
        .collect()
}

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod tests;
