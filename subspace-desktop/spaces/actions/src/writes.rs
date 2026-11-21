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

    /// The writes on the selected run or review, in the inspector's footer
    /// in this order: the review first, as the decision a waiting run asks
    /// for, then the run's own writes. Trigger starts a new run and stays in
    /// the toolbar.
    pub(crate) const FOOTER: [Self; 5] = [
        Self::Approve,
        Self::Reject,
        Self::Rerun,
        Self::RerunFailed,
        Self::Cancel,
    ];

    /// The title with the ellipsis of a control that opens the confirmation,
    /// such as "Trigger…".
    pub(crate) fn prompt(self) -> String {
        format!("{}…", self.title())
    }

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

/// Whether `operation` is the footer's one primary action for `record`:
/// Approve, when a review is what the run waits for.
pub(crate) fn is_primary(operation: Operation, record: Option<&Record>) -> bool {
    operation == Operation::Approve && record.is_some_and(|record| record.approval.is_some())
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
    use super::{Operation, is_primary, unavailable};
    use crate::model::Record;

    #[test]
    fn the_footer_leads_with_the_review_and_holds_every_write_on_the_run() {
        let mut footer = Operation::FOOTER.to_vec();
        assert_eq!(footer[..2], [Operation::Approve, Operation::Reject]);
        footer.sort_by_key(|operation| Operation::ALL.iter().position(|o| o == operation));
        assert_eq!(footer[..], Operation::ALL[1..], "every write but Trigger");
    }

    #[test]
    fn approve_is_primary_only_when_a_review_waits() {
        let mut run = crate::test_support::run("octo/app");
        assert!(!is_primary(Operation::Approve, Some(&run)), "nothing waits");
        assert!(!is_primary(Operation::Approve, None));
        run.approval = Some((7, false));
        assert!(is_primary(Operation::Approve, Some(&run)));
        let others = Operation::FOOTER[1..].iter();
        assert!(
            others
                .clone()
                .all(|operation| !is_primary(*operation, Some(&run)))
        );
        run.approval = Some((7, true));
        assert_eq!(others.filter(|o| is_primary(**o, Some(&run))).count(), 0);
    }

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

    #[test]
    fn every_write_but_trigger_sits_with_the_selection() {
        assert_eq!(Operation::ALL[0], Operation::Dispatch);
        assert!(!Operation::FOOTER.contains(&Operation::Dispatch));
        assert_eq!(Operation::Dispatch.prompt(), "Trigger…");
        assert_eq!(Operation::RerunFailed.prompt(), "Re-run failed…");
    }

    #[test]
    fn the_selected_run_decides_which_writes_apply() {
        let mut run = crate::test_support::run("octo/app");
        let allowed = |record: &Record| {
            Operation::ALL.map(|operation| unavailable(operation, Some(record)).is_none())
        };
        assert_eq!(
            allowed(&run),
            [true, true, true, false, false, false],
            "a finished run: no cancel, and no review without a request"
        );
        run.active = true;
        run.approval = Some((7, true));
        assert_eq!(
            allowed(&run),
            [true; 6],
            "a waiting review this account may give"
        );
        run.approval = Some((7, false));
        assert_eq!(
            unavailable(Operation::Approve, Some(&run)),
            Some("This account cannot review this environment")
        );
        run.run = None;
        assert_eq!(allowed(&run), [false; 6], "an item with no run");
    }
}
