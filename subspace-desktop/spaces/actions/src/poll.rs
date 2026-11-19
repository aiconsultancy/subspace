//! Visible-only polling reads only active runs and their pending reviews.
use subspace_framework::client_github::actions::ActionsApi as _;

use gpui_kit::{AppContext as _, Context};
use std::time::{Duration, Instant};
use subspace_framework::client_github::Client;
use subspace_framework::core::task_state::POLL_INTERVAL;
use subspace_framework::dashboard::refresh;

use crate::{
    Actions,
    fetch::Fetch,
    load::mapping,
    model::{Record, RunTarget, Snapshot},
};

struct Update {
    run: Record,
    approvals: Vec<Record>,
}

pub(crate) fn start_poll(board: &mut Actions, cx: &mut Context<Actions>) {
    board.poll = Some(cx.spawn(async move |this, cx| {
        let mut last_poll = Instant::now();
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let result = this.update(cx, |this, cx| {
                if !this.visible {
                    return false;
                }
                let interval =
                    refresh::interval("actions", POLL_INTERVAL, cx).max(this.backoff.interval());
                // Give the shell's scheduled full refresh the next idle slot.
                if refresh::enabled("actions", cx)
                    && this.fetch.is_idle()
                    && !this.full_load_due(cx)
                    && last_poll.elapsed() >= interval
                {
                    poll_active(this, cx);
                    last_poll = Instant::now();
                }
                true
            });
            if !matches!(result, Ok(true)) {
                return;
            }
        }
    }));
}

fn poll_active(this: &mut Actions, cx: &mut Context<Actions>) {
    if !this.fetch.is_idle() || !refresh::enabled("actions", cx) {
        return;
    }
    let targets = this
        .snapshot()
        .into_iter()
        .flat_map(|data| &data.runs)
        .filter(|run| run.active)
        .filter_map(|run| run.run.clone())
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return;
    }
    refresh_runs(this, targets, cx);
}

/// Refreshes `targets`, replacing any running refresh. Callers start it only
/// when no discovery or full load runs.
pub(crate) fn refresh_runs(
    board: &mut Actions,
    targets: Vec<RunTarget>,
    cx: &mut Context<Actions>,
) {
    let (Some(client), Some(login)) = (board.client.clone(), board.login.clone()) else {
        return;
    };
    let generation = board.generation;
    board.fetch = Fetch::refreshing(cx.spawn(async move |this, cx| {
        let updates = cx
            .background_spawn(async move {
                targets
                    .iter()
                    .map(|target| fetch_update(&client, &login, target))
                    .collect::<Vec<_>>()
            })
            .await;
        _ = this.update(cx, |this, cx| apply_updates(this, generation, updates, cx));
    }));
}

fn apply_updates(
    this: &mut Actions,
    generation: u64,
    updates: Vec<Result<Update, String>>,
    cx: &mut Context<Actions>,
) {
    if generation != this.generation {
        return;
    }
    this.fetch.finish();
    let Some(login) = &this.login else {
        this.rebuild(cx);
        return;
    };
    let data = this.data.entry(login.to_lowercase()).or_default();
    let errors = apply(data, updates);
    this.error = (!errors.is_empty()).then(|| errors.join("\n"));
    if this.error.is_some() {
        this.backoff.failed();
    } else {
        this.backoff.succeeded();
        this.loaded_at = Some(std::time::Instant::now());
    }
    if this.visible
        && let Some(target) = this.detail.target.clone()
    {
        this.load_detail(target, cx);
    }
    this.rebuild(cx);
    this.load_queued_repos(cx);
}

fn fetch_update(client: &Client, login: &str, target: &RunTarget) -> Result<Update, String> {
    let run = client
        .actions_run(login, &target.repo, target.id)
        .map_err(|error| error.to_string())?;
    let pending = client
        .actions_pending_deployments(login, &target.repo, target.id)
        .map_err(|error| error.to_string())?;
    Ok(Update {
        run: mapping::run(&target.repo, &run),
        approvals: pending
            .iter()
            .map(|pending| mapping::approval(&target.repo, &run, pending))
            .collect(),
    })
}

fn apply(data: &mut Snapshot, updates: Vec<Result<Update, String>>) -> Vec<String> {
    let mut errors = data.errors.values().cloned().collect::<Vec<_>>();
    for update in updates {
        match update {
            Ok(update) => {
                data.deployments.retain(|record| {
                    record.approval.is_none()
                        || record.run.as_ref().map(|run| (&run.repo, run.id))
                            != update.run.run.as_ref().map(|run| (&run.repo, run.id))
                });
                if let Some(record) = data
                    .runs
                    .iter_mut()
                    .find(|record| record.key == update.run.key)
                {
                    *record = update.run;
                } else {
                    data.runs.push(update.run);
                }
                data.deployments.extend(update.approvals);
            }
            Err(error) => errors.push(error),
        }
    }
    errors
}

#[cfg(test)]
mod refresh_tests;

#[cfg(test)]
mod tests {
    use super::{Update, apply};
    use crate::{
        load::mapping,
        model::{RunTarget, Snapshot},
    };

    #[test]
    fn refresh_keeps_order_removes_old_attempt_reviews_and_retains_source_errors() {
        let mut first = mapping::notice("octo/app", "first", "First");
        first.run = Some(RunTarget {
            repo: "octo/app".into(),
            id: 1,
            workflow_id: 2,
            workflow: "CI".into(),
            path: String::new(),
            sha: String::new(),
            branch: "main".into(),
            actor: "octocat".into(),
            attempt: 1,
        });
        let second = mapping::notice("octo/app", "second", "Second");
        let mut review = first.clone();
        review.approval = Some((3, true));
        let mut updated = first.clone();
        updated.run.as_mut().unwrap().attempt = 2;
        let mut data = Snapshot {
            runs: vec![first, second],
            deployments: vec![review],
            errors: [("Releases".into(), "Release access denied".into())].into(),
            ..Default::default()
        };
        let errors = apply(
            &mut data,
            vec![Ok(Update {
                run: updated,
                approvals: Vec::new(),
            })],
        );
        assert_eq!(data.runs[0].title, "First");
        assert_eq!(data.runs[1].title, "Second");
        assert_eq!(data.runs[0].run.as_ref().unwrap().attempt, 2);
        assert!(data.deployments.is_empty());
        assert_eq!(errors, ["Release access denied"]);
    }
}
