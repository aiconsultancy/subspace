//! Blocking read composition. Called only by background tasks.
use subspace_framework::client_github::actions::ActionsApi as _;

use subspace_framework::client_github::{
    Client,
    actions::{RunFilters, WorkflowRun},
};

use super::{RepoResult, mapping};
use crate::model::{Record, active};

pub(super) fn repository(client: &Client, login: &str, repo: &str) -> RepoResult {
    let runs = client.actions_runs(login, repo, &RunFilters::default());
    let known = runs
        .as_ref()
        .map(|page| page.items.as_slice())
        .unwrap_or_default();
    let deployments = deployments(client, login, repo, known);
    let releases = releases(client, login, repo, known);
    let runs = runs
        .map(|page| {
            let mut rows = page
                .items
                .iter()
                .map(|run| mapping::run(repo, run))
                .collect::<Vec<_>>();
            if page.truncated {
                rows.push(mapping::notice(
                    repo,
                    "runs-truncated",
                    "Run history is truncated to the client page limit",
                ));
            }
            rows
        })
        .map_err(|error| error.to_string());
    RepoResult {
        repo: repo.into(),
        runs,
        deployments,
        releases,
    }
}

fn deployments(
    client: &Client,
    login: &str,
    repo: &str,
    runs: &[WorkflowRun],
) -> Result<Vec<Record>, String> {
    let environments = client
        .actions_environments(login, repo)
        .map_err(|error| error.to_string())?;
    let mut records = Vec::new();
    for environment in environments.items {
        let deployment = client
            .actions_latest_deployment(login, repo, &environment.name)
            .map_err(|error| error.to_string())?;
        records.push(mapping::environment(
            repo,
            &environment,
            deployment.as_ref(),
        ));
    }
    if records.is_empty() {
        records.push(mapping::notice(repo, "no-environments", "No environments"));
    }
    if environments.truncated {
        records.push(mapping::notice(
            repo,
            "environments-truncated",
            "Environment list is truncated",
        ));
    }
    records.extend(approvals(client, login, repo, runs)?);
    Ok(records)
}

pub(super) fn approvals(
    client: &Client,
    login: &str,
    repo: &str,
    runs: &[WorkflowRun],
) -> Result<Vec<Record>, String> {
    let mut records = Vec::new();
    for run in runs.iter().filter(|run| active(&run.status)) {
        let pending = client
            .actions_pending_deployments(login, repo, run.id)
            .map_err(|error| error.to_string())?;
        records.extend(
            pending
                .iter()
                .map(|pending| mapping::approval(repo, run, pending)),
        );
    }
    Ok(records)
}

fn releases(
    client: &Client,
    login: &str,
    repo: &str,
    runs: &[WorkflowRun],
) -> Result<Vec<Record>, String> {
    let releases = client
        .actions_releases(login, repo)
        .map_err(|error| error.to_string())?;
    let tags = client
        .actions_tags(login, repo)
        .map_err(|error| error.to_string())?;
    let mut records = releases
        .items
        .iter()
        .map(|release| {
            let sha = tags
                .items
                .iter()
                .find(|tag| tag.name == release.tag_name)
                .map(|tag| &tag.commit.sha);
            let completed = runs
                .iter()
                .any(|run| run.status == "completed" && Some(&run.head_sha) == sha);
            mapping::release(repo, release, completed)
        })
        .collect::<Vec<_>>();
    for tag in tags.items.iter().filter(|tag| {
        !releases
            .items
            .iter()
            .any(|release| release.tag_name == tag.name)
    }) {
        records.push(mapping::notice(
            repo,
            &format!("tag-{}", tag.name),
            &format!("{} · tag without release", tag.name),
        ));
    }
    if releases.truncated || tags.truncated {
        records.push(mapping::notice(
            repo,
            "releases-truncated",
            "Release/tag history is truncated to one page",
        ));
    }
    Ok(records)
}
