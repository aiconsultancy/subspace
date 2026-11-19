//! Write intent and confirmation lifecycle. Render never sends a request.

use gpui_kit::AppContext as _;
use gpui_kit::{Context, Window};
use std::collections::BTreeMap;
use subspace_framework::client_github::actions::ActionsApi as _;
use subspace_framework::client_github::{
    Client,
    actions::{DispatchRequest, ReviewPendingRequest, ReviewState},
};
use subspace_framework::dashboard::{DashboardEvent, Toast};
use subspace_framework::lucide::IconName;

use crate::{Actions, dialogs::Confirmation, model::Record};

struct Submission {
    operation: Operation,
    record: Record,
    reference: String,
    inputs: BTreeMap<String, String>,
    comment: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Dispatch,
    Rerun,
    RerunFailed,
    Cancel,
    Approve,
    Reject,
}

impl Operation {
    pub(crate) const ALL: [Self; 6] = [
        Self::Dispatch,
        Self::Rerun,
        Self::RerunFailed,
        Self::Cancel,
        Self::Approve,
        Self::Reject,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Dispatch => "Trigger",
            Self::Rerun => "Re-run",
            Self::RerunFailed => "Re-run failed",
            Self::Cancel => "Cancel run",
            Self::Approve => "Approve",
            Self::Reject => "Reject",
        }
    }

    pub(crate) fn icon(self) -> IconName {
        match self {
            Self::Dispatch => IconName::Play,
            Self::Rerun | Self::RerunFailed => IconName::RotateCw,
            Self::Cancel | Self::Reject => IconName::X,
            Self::Approve => IconName::Check,
        }
    }
}

pub(crate) fn unavailable(operation: Operation, record: Option<&Record>) -> Option<&'static str> {
    let Some(record) = record else {
        return Some("Select a run or pending deployment first");
    };
    let Some(run) = &record.run else {
        return Some("This item has no workflow run");
    };
    match operation {
        Operation::Cancel if !record.active => Some("A completed run cannot be cancelled"),
        Operation::Rerun | Operation::RerunFailed if run.attempt == 0 => {
            Some("The run has no attempt to re-run")
        }
        Operation::Approve | Operation::Reject
            if !record.approval.is_some_and(|(_, allowed)| allowed) =>
        {
            Some("This account cannot review this environment")
        }
        _ => None,
    }
}

impl Actions {
    pub(crate) fn confirm_write(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = &self.pending else { return };
        if pending.busy || pending.definition.blocks_submit() {
            return;
        }
        let submission = match submission(pending, cx) {
            Ok(value) => value,
            Err(error) => {
                if let Some(pending) = &mut self.pending {
                    pending.error = Some(error);
                }
                cx.notify();
                return;
            }
        };
        let (Some(client), Some(login)) = (self.client.clone(), self.login.clone()) else {
            return;
        };
        if let Some(pending) = &mut self.pending {
            pending.busy = true;
            pending.error = None;
        }
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { execute(&client, &login, &submission) })
                .await;
            _ = this.update(cx, |this, cx| this.write_finished(generation, result, cx));
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn write_finished(
        &mut self,
        generation: u64,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        }
        let Some(pending) = &mut self.pending else {
            return;
        };
        pending.busy = false;
        match result {
            Err(error) => pending.error = Some(error),
            Ok(()) => {
                let target = pending.record.run.clone();
                let link = pending.record.link.clone();
                self.pending = None;
                cx.emit(DashboardEvent::Toast(
                    Toast::info("GitHub accepted the request").link(link, "Open run"),
                ));
                if let Some(target) = target.and_then(|target| self.fetch.after_write(target)) {
                    crate::poll::refresh_runs(self, vec![target], cx);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn begin_write(
        &mut self,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending.is_some() {
            return;
        }
        if let Some(reason) = unavailable(operation, self.selected()) {
            cx.emit(DashboardEvent::Toast(Toast::error(reason)));
            return;
        }
        let Some(record) = self.selected().cloned() else {
            return;
        };
        self.filter_popup = None;
        self.pending = Some(Confirmation::new(operation, record, window, cx));
        if operation == Operation::Dispatch {
            crate::dialogs::load_definition(self, window, cx);
        }
        cx.notify();
    }
}

fn submission(pending: &Confirmation, cx: &gpui_kit::App) -> Result<Submission, String> {
    let reference = pending.reference.read(cx).value().to_string();
    let comment = pending.comment.read(cx).value().to_string();
    if pending.operation == Operation::Dispatch && reference.trim().is_empty() {
        return Err("A branch or tag is required".into());
    }
    if pending.operation == Operation::Reject && comment.trim().is_empty() {
        return Err("A rejection comment is required".into());
    }
    let inputs = if pending.operation == Operation::Dispatch {
        crate::dialogs::values(pending, cx)?
    } else {
        BTreeMap::new()
    };
    Ok(Submission {
        operation: pending.operation,
        record: pending.record.clone(),
        reference,
        inputs,
        comment,
    })
}

fn execute(client: &Client, login: &str, submission: &Submission) -> Result<(), String> {
    let target = submission
        .record
        .run
        .as_ref()
        .ok_or("The run target is missing")?;
    let result = match submission.operation {
        Operation::Dispatch => client.dispatch_workflow(
            login,
            &DispatchRequest {
                repo: &target.repo,
                workflow: &target.workflow_id.to_string(),
                reference: &submission.reference,
                inputs: &submission.inputs,
            },
        ),
        Operation::Rerun => client.rerun_workflow(login, &target.repo, target.id),
        Operation::RerunFailed => client.rerun_failed_jobs(login, &target.repo, target.id),
        Operation::Cancel => client.cancel_workflow(login, &target.repo, target.id),
        Operation::Approve | Operation::Reject => {
            let (environment, _) = submission
                .record
                .approval
                .ok_or("The environment target is missing")?;
            let state = if submission.operation == Operation::Approve {
                ReviewState::Approved
            } else {
                ReviewState::Rejected
            };
            client.review_pending_deployments(
                login,
                &ReviewPendingRequest {
                    repo: &target.repo,
                    run_id: target.id,
                    environment_ids: &[environment],
                    state,
                    comment: &submission.comment,
                },
            )
        }
    };
    result.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Operation, unavailable};

    #[test]
    fn writes_require_a_selected_target() {
        for operation in Operation::ALL {
            assert_eq!(
                unavailable(operation, None),
                Some("Select a run or pending deployment first")
            );
            assert!(!operation.title().is_empty());
        }
    }
}
