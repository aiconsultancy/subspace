//! Background job and dependency reads for one open run.
use subspace_framework::client_github::actions::ActionsApi as _;

use gpui_kit::{AppContext as _, Context};
use subspace_framework::client_github::{
    Client,
    actions::{Definition, Job, match_job_dependencies},
};

use crate::{Actions, graph::GraphJob, model::RunTarget};

struct Loaded {
    jobs: Vec<GraphJob>,
    edges: Vec<(u64, u64)>,
    reason: Option<String>,
}

impl Actions {
    pub(crate) fn load_job_log(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(target) = self.detail.target.clone() else {
            return;
        };
        let (Some(client), Some(login)) = (self.client.clone(), self.login.clone()) else {
            return;
        };
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let repo = target.repo.clone();
            let result = cx
                .background_spawn(async move { client.actions_job_log(&login, &repo, id) })
                .await;
            _ = this.update(cx, |this, cx| {
                if generation != this.generation
                    || this.detail.target.as_ref() != Some(&target)
                    || this.list.open_row().is_none()
                {
                    return;
                }
                match result {
                    Ok(log) => this.detail.log = Some(log.text),
                    Err(error) => this.detail.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn load_detail(&mut self, target: RunTarget, cx: &mut Context<Self>) {
        let (Some(client), Some(login)) = (self.client.clone(), self.login.clone()) else {
            return;
        };
        if self.detail.loading && self.detail.target.as_ref() == Some(&target) {
            return;
        }
        if self.detail.target.as_ref() != Some(&target) {
            self.detail = Default::default();
            self.graph = crate::panel::new_graph(cx);
        }
        self.detail.target = Some(target.clone());
        self.detail.loading = true;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let fetch_target = target.clone();
            let result = cx
                .background_spawn(async move { fetch(&client, &login, &fetch_target) })
                .await;
            _ = this.update(cx, |this, cx| {
                if generation != this.generation || this.detail.target.as_ref() != Some(&target) {
                    return;
                }
                this.detail.loading = false;
                match result {
                    Ok(loaded) => {
                        this.detail.jobs = loaded.jobs.clone();
                        this.detail.error = None;
                        this.graph.update(cx, |graph, cx| {
                            graph.set_jobs(loaded.jobs, loaded.edges, loaded.reason, cx)
                        });
                    }
                    Err(error) => this.detail.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn fetch(client: &Client, login: &str, target: &RunTarget) -> Result<Loaded, String> {
    let jobs = client
        .actions_jobs(login, &target.repo, target.id)
        .map_err(|error| error.to_string())?;
    let path = target.path.split('@').next().unwrap_or(&target.path);
    let definition = client.actions_workflow_definition(login, &target.repo, &target.sha, path);
    let dependencies = match definition {
        Ok(Definition::Readable(definition)) => {
            match_job_dependencies(&jobs.items, &definition.jobs)
        }
        Ok(Definition::Unreadable(reason)) => Definition::Unreadable(reason),
        Err(error) => Definition::Unreadable(error.to_string()),
    };
    let (edges, mut reason) = match dependencies {
        Definition::Readable(dependencies) => (
            dependencies
                .iter()
                .flat_map(|job| {
                    job.needs
                        .iter()
                        .map(|predecessor| (*predecessor, job.job_id))
                })
                .collect(),
            None,
        ),
        Definition::Unreadable(reason) => (Vec::new(), Some(reason)),
    };
    if jobs.truncated {
        reason = Some("Jobs are truncated; dependencies may be incomplete".into());
    }
    Ok(Loaded {
        jobs: jobs.items.iter().map(graph_job).collect(),
        edges,
        reason,
    })
}

fn graph_job(job: &Job) -> GraphJob {
    let steps = job
        .steps
        .iter()
        .map(|step| {
            format!(
                "{}. {} · {} · {}",
                step.number,
                step.name,
                step.conclusion.as_deref().unwrap_or(&step.status),
                duration(step.started_at.as_deref(), step.completed_at.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    GraphJob {
        id: job.id,
        name: job.name.clone(),
        status: job.conclusion.as_deref().unwrap_or(&job.status).into(),
        duration: duration(job.started_at.as_deref(), job.completed_at.as_deref()),
        steps: format!(
            "Runner: {}\nLabels: {}\n\n{steps}",
            job.runner_name.as_deref().unwrap_or("Not assigned"),
            job.labels.join(", ")
        ),
    }
}

fn duration(start: Option<&str>, end: Option<&str>) -> String {
    let start = start.and_then(subspace_framework::dashboard::dates::parse_date);
    let end = end.and_then(subspace_framework::dashboard::dates::parse_date);
    match (start, end) {
        (Some(start), Some(end)) => format!("{}s", (end - start).num_seconds().max(0)),
        (Some(_), None) => "Running".into(),
        _ => "Not started".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::duration;

    #[test]
    fn durations_distinguish_queued_running_and_finished_jobs() {
        assert_eq!(duration(None, None), "Not started");
        assert_eq!(duration(Some("2026-10-24T12:00:00Z"), None), "Running");
        assert_eq!(
            duration(Some("2026-10-24T12:00:00Z"), Some("2026-10-24T12:01:00Z")),
            "60s"
        );
    }
}
