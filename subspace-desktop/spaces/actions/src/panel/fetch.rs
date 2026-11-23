//! Background job, log and dependency reads for the selected and opened
//! runs. Every read carries a token; an answer is kept only by the copy of
//! the run's detail that still waits for that very read, so an earlier
//! selection, attempt or log request never lands on a later one. One jobs
//! read runs at a time.
use subspace_framework::client_github::actions::ActionsApi as _;

use gpui_kit::{AppContext as _, Context};
use subspace_framework::client_github::{
    Client,
    actions::{Definition, Job, match_job_dependencies},
};

use crate::panel::{LogState, RunDetail, same_run};
use crate::{Actions, graph::GraphJob, model::RunTarget};

struct Loaded {
    jobs: Vec<GraphJob>,
    edges: Vec<(u64, u64)>,
    reason: Option<String>,
}

impl Actions {
    /// A new token for one jobs or log read.
    fn next_request(&mut self) -> u64 {
        self.requests = self.requests.wrapping_add(1);
        self.requests
    }

    /// The selection's jobs, while they are the selected run's at the
    /// attempt it now stands at. None while they still belong to an earlier
    /// selection, or to an earlier attempt of the same run, such as when a
    /// refresh moved the run on before its jobs were read again.
    pub(crate) fn selected_detail(&self) -> Option<&RunDetail> {
        let run = self.selected()?.run.as_ref()?;
        (self.detail.target.as_ref() == Some(run)).then_some(&self.detail)
    }

    /// Reads job `id`'s log for the selected run and shows it in that run's
    /// tab, which it opens. The job must be one of the jobs loaded for the
    /// selected run's current attempt, so a pick offered for an earlier
    /// selection or attempt reads nothing. Only the tab's newest log read is
    /// shown, and a failed read says so where the log goes, keeping the jobs.
    pub(crate) fn load_job_log(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(record) = self.selected().cloned() else {
            return;
        };
        let Some(target) = record.run.clone() else {
            return;
        };
        let known = self
            .selected_detail()
            .is_some_and(|detail| detail.jobs.iter().any(|job| job.id == id));
        if !known {
            return;
        }
        let (Some(client), Some(login)) = (self.client.clone(), self.login.clone()) else {
            return;
        };
        self.open_tab(&record.key, cx);
        let request = self.next_request();
        let Some(open) = self.open.as_mut().filter(|open| open.key == record.key) else {
            return;
        };
        open.detail.log = LogState::Loading;
        open.detail.log_request = request;
        let generation = self.generation;
        let key = record.key;
        cx.spawn(async move |this, cx| {
            let repo = target.repo.clone();
            let result = cx
                .background_spawn(async move { client.actions_job_log(&login, &repo, id) })
                .await
                .map(|log| log.text)
                .map_err(|error| error.to_string());
            _ = this.update(cx, |this, cx| {
                this.finish_job_log(generation, &key, request, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    /// Puts a log answer in the opened tab, unless the tab closed, holds
    /// another item, or asked for a newer log since.
    fn finish_job_log(
        &mut self,
        generation: u64,
        key: &str,
        request: u64,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.key == key && open.detail.log_request == request)
        else {
            return;
        };
        open.detail.log = match result {
            Ok(text) => LogState::Loaded(text),
            Err(error) => LogState::Failed(error),
        };
        cx.notify();
    }

    /// Reads `target`'s jobs for the selection. The opened tab on the same
    /// run takes the same read; a new attempt starts the tab afresh, without
    /// the earlier attempt's jobs or log.
    pub(crate) fn load_detail(&mut self, target: RunTarget, cx: &mut Context<Self>) {
        if self.client.is_none() || self.login.is_none() {
            return;
        }
        if self.detail.loading && self.detail.target.as_ref() == Some(&target) {
            return;
        }
        if self.detail.target.as_ref() != Some(&target) {
            self.detail = RunDetail::default();
            self.graph = crate::panel::new_graph(cx);
        }
        let request = self.next_request();
        self.detail.target = Some(target.clone());
        self.detail.loading = true;
        self.detail.request = request;
        if let Some(open) = self.open.as_mut()
            && open
                .detail
                .target
                .as_ref()
                .is_some_and(|shown| same_run(shown, &target))
        {
            if open.detail.target.as_ref() != Some(&target) {
                open.detail = RunDetail {
                    target: Some(target),
                    loading: true,
                    ..RunDetail::default()
                };
            }
            open.detail.request = request;
        }
        self.start_detail_read(cx);
        cx.notify();
    }

    /// Reads the opened run again when its record moved to another attempt
    /// that its tab has not read, such as after a re-run while another run
    /// is selected.
    pub(crate) fn refresh_open_detail(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.open else {
            return;
        };
        let current = self
            .records_in(open.view)
            .iter()
            .find(|record| record.key == open.key)
            .and_then(|record| record.run.clone());
        let Some(current) = current.filter(|run| open.detail.target.as_ref() != Some(run)) else {
            return;
        };
        let request = self.next_request();
        if let Some(open) = self.open.as_mut() {
            open.detail = RunDetail {
                target: Some(current),
                loading: true,
                request,
                ..RunDetail::default()
            };
        }
        self.start_detail_read(cx);
        cx.notify();
    }

    /// Starts the next waiting jobs read unless one runs: the selection's
    /// first, then the opened tab's. Rows passed over while a read ran are
    /// never read, so quick selection starts one read, not one per row.
    fn start_detail_read(&mut self, cx: &mut Context<Self>) {
        if self.reading.is_some() {
            return;
        }
        let (Some(client), Some(login)) = (self.client.clone(), self.login.clone()) else {
            return;
        };
        let waiting = [
            Some(&self.detail),
            self.open.as_ref().map(|open| &open.detail),
        ]
        .into_iter()
        .flatten()
        .filter(|detail| detail.loading)
        .find_map(|detail| Some((detail.target.clone()?, detail.request)));
        let Some((target, request)) = waiting else {
            return;
        };
        self.reading = Some(request);
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { fetch(&client, &login, &target) })
                .await;
            _ = this.update(cx, |this, cx| {
                this.finish_detail(generation, request, result, cx);
            });
        })
        .detach();
    }

    /// Puts a jobs answer into each copy of the detail that waits for it,
    /// then starts the read that waits next.
    fn finish_detail(
        &mut self,
        generation: u64,
        request: u64,
        result: Result<Loaded, String>,
        cx: &mut Context<Self>,
    ) {
        if self.reading == Some(request) {
            self.reading = None;
        }
        if generation == self.generation {
            if let Some(open) = self.open.as_mut()
                && open.detail.request == request
            {
                finish(&mut open.detail, &result);
            }
            if self.detail.request == request {
                finish(&mut self.detail, &result);
                if let Ok(loaded) = result {
                    self.graph.update(cx, |graph, cx| {
                        graph.set_jobs(loaded.jobs, loaded.edges, loaded.reason, cx)
                    });
                }
            }
        }
        self.start_detail_read(cx);
        cx.notify();
    }
}

/// Puts a finished read into one copy of a run's detail: the selection's,
/// or the opened tab's. The log is left as it is.
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
    // The attempt the run was read at: a re-run started since has jobs of
    // its own, which must not answer for this attempt.
    let jobs = client
        .actions_attempt_jobs(login, &target.repo, target.id, target.attempt)
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

#[cfg(test)]
mod runtime_tests;
