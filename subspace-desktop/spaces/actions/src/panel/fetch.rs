//! Background job and dependency reads for one open run.
use subspace_framework::client_github::actions::ActionsApi as _;

use gpui_kit::{AppContext as _, Context};
use subspace_framework::client_github::{
    Client,
    actions::{Definition, Job, match_job_dependencies},
};

use crate::{Actions, graph::GraphJob, model::RunTarget, panel::RunDetail};

struct Loaded {
    jobs: Vec<GraphJob>,
    edges: Vec<(u64, u64)>,
    reason: Option<String>,
}

impl Actions {
    /// Reads job `id`'s log for the selected run and shows it in that run's
    /// tab, which it opens. A log that arrives after the tab closed, or
    /// moved to another run, is dropped.
    pub(crate) fn load_job_log(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(target) = self.detail.target.clone() else {
            return;
        };
        let (Some(client), Some(login)) = (self.client.clone(), self.login.clone()) else {
            return;
        };
        let key = self
            .records()
            .iter()
            .find(|record| record.run.as_ref() == Some(&target))
            .map(|record| record.key.clone());
        if let Some(key) = key {
            self.open_tab(&key, cx);
        }
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let repo = target.repo.clone();
            let result = cx
                .background_spawn(async move { client.actions_job_log(&login, &repo, id) })
                .await;
            _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                let Some(open) = this.open.as_mut() else {
                    return;
                };
                if open.detail.target.as_ref() != Some(&target) {
                    return;
                }
                match result {
                    Ok(log) => open.detail.log = Some(log.text),
                    Err(error) => open.detail.error = Some(error.to_string()),
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
                if generation != this.generation {
                    return;
                }
                if let Some(open) = this.open.as_mut()
                    && open.detail.target.as_ref() == Some(&target)
                {
                    finish(&mut open.detail, &result);
                }
                if this.detail.target.as_ref() != Some(&target) {
                    cx.notify();
                    return;
                }
                finish(&mut this.detail, &result);
                if let Ok(loaded) = result {
                    this.graph.update(cx, |graph, cx| {
                        graph.set_jobs(loaded.jobs, loaded.edges, loaded.reason, cx)
                    });
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

/// Puts a finished read into one copy of a run's detail: the selection's,
/// or the opened tab's.
fn finish(detail: &mut RunDetail, result: &Result<Loaded, String>) {
    detail.loading = false;
    match result {
        Ok(loaded) => {
            detail.jobs = loaded.jobs.clone();
            detail.error = None;
        }
        Err(error) => detail.error = Some(error.clone()),
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
