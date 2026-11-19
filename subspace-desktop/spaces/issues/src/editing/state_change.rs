//! The issue page's end-state menu changes one property: the state. It sends
//! the editor's sparse PATCH against the loaded issue's version, then reloads
//! the issue and the list as a saved edit does. Subspace API stays the
//! authority: the page shows the new state only once the reload returns it,
//! and the end slot stays busy until then.

use gpui_kit::{AppContext as _, Context};
use subspace_api_client::{
    hub::{Hub, IssueStatus, IssueWriteScope},
    issue_writes::{IssueChanges, IssueProperty, IssueSave, IssueSaveError},
    model::IssueDetail,
};
use subspace_framework::dashboard::{DashboardEvent, Toast};

use super::target::editable_detail;
use crate::Issues;
use crate::detail::StateChange;

/// One state change for the open issue, ready to send.
struct StateWrite {
    scope: IssueWriteScope,
    issue: String,
    identifier: String,
    version: u32,
    changes: IssueChanges,
}

/// A sent state change: which issue, and what Subspace API answered.
struct StateWritten {
    issue: String,
    identifier: String,
    result: Result<IssueSave, IssueSaveError>,
}

impl StateWrite {
    /// The write that moves `detail` to `state`, unless it is a tracker, whose
    /// state Subspace API derives, or the issue is already there.
    fn new(scope: IssueWriteScope, detail: &IssueDetail, state: String) -> Option<Self> {
        let fields = &detail.fields;
        if fields.kind == "tracker" || fields.state.id == state {
            return None;
        }
        let mut changes = IssueChanges::default();
        changes.set(IssueProperty::State(state));
        Some(Self {
            scope,
            issue: fields.id.clone(),
            identifier: fields.identifier.clone(),
            version: fields.version.try_into().ok()?,
            changes,
        })
    }

    fn send(self, hub: &Hub) -> StateWritten {
        let result = hub.update_issue(&self.scope, &self.issue, self.version, &self.changes);
        StateWritten {
            issue: self.issue,
            identifier: self.identifier,
            result,
        }
    }
}

// qual:allow(coupling, oi) reason: "The end-state menu dispatches one scoped write and applies its outcome to the board."
impl Issues {
    /// Moves the open issue to the workflow state `state`. A change already
    /// on its way, a read-only connection or a tracker sends nothing.
    pub(crate) fn change_issue_state(&mut self, state: String, cx: &mut Context<Self>) {
        let Some((hub, write)) = self.state_write(state) else {
            return;
        };
        self.issue_view.state_change = Some(StateChange {
            issue: write.issue.clone(),
            reload_after: None,
            written_version: None,
        });
        cx.spawn(async move |this, cx| {
            let written = cx.background_spawn(async move { write.send(&hub) }).await;
            _ = this.update(cx, |this, cx| this.finish_state_change(written, cx));
        })
        .detach();
        cx.notify();
    }

    /// Whether the open issue's end-state change is on its way, or its
    /// reload has not yet returned.
    pub(crate) fn changing_state(&self, issue: &str) -> bool {
        self.issue_view
            .state_change
            .as_ref()
            .is_some_and(|change| change.issue == issue)
    }

    /// Ends the change once the reload it asked for has settled: a newer read
    /// of the issue that shows the written version, or a failed read.
    pub(crate) fn settle_state_change(&mut self) {
        let Some(change) = self.issue_view.state_change.as_ref() else {
            return;
        };
        let (Some(before), Some(open)) = (change.reload_after, &self.issue_view.snapshot) else {
            return;
        };
        let settled = match open.status() {
            IssueStatus::Loading => false,
            IssueStatus::Loaded => {
                open.updated_at() != before
                    && change.written_version.is_none_or(|version| {
                        open.detail()
                            .is_some_and(|detail| detail.fields.version >= version)
                    })
            }
            _ => true,
        };
        if settled {
            self.issue_view.state_change = None;
        }
    }

    /// Whether `issue` is still the open issue with this change on its way.
    /// If so, the change now waits for the reload of the issue as it is, and
    /// for `written_version` when the write was saved.
    fn await_reload(&mut self, issue: &str, written_version: Option<i64>) -> bool {
        let open = self.issue_view.snapshot.as_ref();
        let current = open
            .and_then(|open| open.detail())
            .is_some_and(|detail| detail.fields.id == issue);
        let before = open.and_then(|open| open.updated_at());
        match &mut self.issue_view.state_change {
            Some(change) if current && change.issue == issue => {
                change.reload_after = Some(before);
                change.written_version = written_version;
                true
            }
            _ => false,
        }
    }

    /// Whether the page shows `issue`: by its id once loaded, or by its
    /// identifier while it loads.
    fn is_open(&self, issue: &str, identifier: &str) -> bool {
        self.issue_view.snapshot.as_ref().is_some_and(|open| {
            open.identifier() == identifier
                || open
                    .detail()
                    .is_some_and(|detail| detail.fields.id == issue)
        })
    }

    fn state_write(&self, state: String) -> Option<(Hub, StateWrite)> {
        if self.issue_view.state_change.is_some() {
            return None;
        }
        let hub = self.hub.clone()?;
        let scope = hub.issue_write_scope()?;
        let write = StateWrite::new(scope, editable_detail(self)?, state)?;
        Some((hub, write))
    }

    /// The write has returned. If its issue is open, reload it; while the
    /// change is still tracked, the end slot stays busy until that reload
    /// settles. An issue left and reopened while the write was on its way is
    /// no longer tracked, but its page may show the state from before the
    /// write, so it reloads too. A result for an issue that is not open still
    /// refreshes the list and still reports a failure, but leaves the open
    /// issue, and its own state, alone.
    fn finish_state_change(&mut self, written: StateWritten, cx: &mut Context<Self>) {
        let written_version = match &written.result {
            Ok(IssueSave::Saved(detail)) => Some(detail.fields.version),
            Err(_) => None,
        };
        let tracked = self.await_reload(&written.issue, written_version);
        if tracked || self.is_open(&written.issue, &written.identifier) {
            self.reload_detail(cx);
        }
        match &written.result {
            Ok(_) => self.reload_list(cx),
            Err(error) => cx.emit(DashboardEvent::Toast(Toast::error(failure(
                &written.identifier,
                error,
            )))),
        }
        cx.notify();
    }
}

/// What the toast says when Subspace API refuses or cannot take the change.
/// The reload that follows shows the issue's latest state.
fn failure(identifier: &str, error: &IssueSaveError) -> String {
    match error {
        IssueSaveError::Conflict => {
            format!("{identifier} changed since it was read. Its latest state is shown.")
        }
        IssueSaveError::IssueArchived => {
            format!("{identifier} is archived. Restore it before changing its state.")
        }
        IssueSaveError::Forbidden => {
            format!("You don't have permission to change the state of {identifier}.")
        }
        IssueSaveError::SignIn => {
            format!(
                "Couldn't change the state of {identifier}. Sign in under Settings › Account, then try again."
            )
        }
        IssueSaveError::Unconfirmed => {
            format!(
                "Couldn't confirm the state change for {identifier}. Check its state before trying again."
            )
        }
        _ => format!("Couldn't change the state of {identifier}. Try again when connected."),
    }
}

#[cfg(test)]
mod tests {
    use super::failure;
    use subspace_api_client::issue_writes::IssueSaveError;

    #[test]
    fn failures_name_the_issue_and_the_way_forward_without_draft_wording() {
        for error in [
            IssueSaveError::Conflict,
            IssueSaveError::IssueArchived,
            IssueSaveError::Forbidden,
            IssueSaveError::SignIn,
            IssueSaveError::Unconfirmed,
            IssueSaveError::NotSent,
            IssueSaveError::Unavailable,
        ] {
            let text = failure("OPS-1", &error);
            assert!(text.contains("OPS-1"), "{text}");
            assert!(!text.contains("draft"), "{text}");
            assert!(text.ends_with('.'), "{text}");
        }
        assert_eq!(
            failure("OPS-1", &IssueSaveError::NotSent),
            "Couldn't change the state of OPS-1. Try again when connected."
        );
    }
}
