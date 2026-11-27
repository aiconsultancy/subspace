//! New children appear immediately, even beyond the parent's first page.
use std::time::Instant;

use gpui_kit::{AppContext as _, Context};
use subspace_api_client::{
    hub::IssueStatus,
    issue_writes::IssueSaveError,
    model::{IssueDetail, IssueFields},
};

use super::IssueView;
use crate::Issues;

#[derive(Default)]
pub(super) struct CreatedSubissues(Vec<Projected>);

struct Projected {
    fields: IssueFields,
    checked: Option<Instant>,
    reading: Option<Instant>,
}

enum ChildRead {
    Current(Box<IssueFields>),
    Gone,
    Unknown,
}

fn classify(read: Result<IssueDetail, IssueSaveError>, id: &str, parent: &str) -> ChildRead {
    match read {
        Ok(issue)
            if issue.fields.id == id
                && issue.fields.archived_at.is_none()
                && issue.fields.parent_id.as_deref() == Some(parent) =>
        {
            ChildRead::Current(Box::new(issue.fields))
        }
        Ok(_) | Err(IssueSaveError::Unavailable | IssueSaveError::Forbidden) => ChildRead::Gone,
        Err(_) => ChildRead::Unknown,
    }
}

impl CreatedSubissues {
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
}

impl IssueView {
    pub(crate) fn project_subissue(&mut self, fields: IssueFields) {
        self.created_subissues.0.push(Projected {
            fields,
            checked: None,
            reading: None,
        });
    }

    pub(crate) fn projected_subissues(&self) -> impl DoubleEndedIterator<Item = &IssueFields> {
        self.created_subissues.0.iter().map(|row| &row.fields)
    }

    fn loaded_at(&self) -> Option<Instant> {
        let issue = self.snapshot.as_ref()?;
        (*issue.status() == IssueStatus::Loaded).then(|| issue.updated_at())?
    }

    #[cfg(test)]
    pub(crate) fn subissue_projection_checked(&self, id: &str) -> bool {
        self.loaded_at().is_some_and(|at| {
            self.created_subissues
                .0
                .iter()
                .any(|row| row.fields.id == id && row.checked == Some(at))
        })
    }
}

// qual:allow(coupling, oi) reason: "This board boundary dispatches scoped child reads and applies results only to the matching open detail refresh."
impl Issues {
    pub(super) fn check_created_subissues(&mut self, cx: &mut Context<Self>) {
        let view = &mut self.issue_view;
        let Some(at) = view.loaded_at() else { return };
        let Some(detail) = view.snapshot.as_ref().and_then(|issue| issue.detail()) else {
            return;
        };
        let Some(page) = detail
            .children
            .as_ref()
            .filter(|page| page.next_cursor.is_some())
        else {
            view.created_subissues.clear();
            return;
        };
        let rows = &mut view.created_subissues.0;
        rows.retain(|row| {
            !page
                .items
                .iter()
                .any(|item| item.fields.id == row.fields.id)
        });
        let Some((hub, scope)) = self
            .hub
            .as_ref()
            .and_then(|hub| hub.issue_write_scope().map(|scope| (hub, scope)))
        else {
            return;
        };
        for row in rows
            .iter_mut()
            .filter(|row| row.reading.is_none() && row.checked != Some(at))
        {
            row.reading = Some(at);
            let (hub, scope, id, parent) = (
                hub.clone(),
                scope.clone(),
                row.fields.id.clone(),
                detail.fields.id.clone(),
            );
            cx.spawn(async move |board, cx| {
                let (id, outcome) = cx
                    .background_spawn(async move {
                        let outcome = classify(hub.latest_issue(&scope, &id), &id, &parent);
                        (id, outcome)
                    })
                    .await;
                let _ = board.update(cx, |board, cx| {
                    board.finish_projected_subissue(&id, at, outcome, cx);
                });
            })
            .detach();
        }
    }

    fn finish_projected_subissue(
        &mut self,
        id: &str,
        at: Instant,
        outcome: ChildRead,
        cx: &mut Context<Self>,
    ) {
        let view = &mut self.issue_view;
        let current = view.loaded_at() == Some(at);
        let rows = &mut view.created_subissues.0;
        let Some(index) = rows
            .iter()
            .position(|row| row.fields.id == id && row.reading == Some(at))
        else {
            return;
        };
        rows[index].reading = None;
        if current {
            rows[index].checked = Some(at);
            match outcome {
                ChildRead::Current(fields) => rows[index].fields = *fields,
                ChildRead::Gone => {
                    rows.remove(index);
                }
                ChildRead::Unknown => {}
            }
            cx.notify();
        } else {
            // A refresh overtook the read. Reconcile once against the new page.
            self.check_created_subissues(cx);
        }
    }
}
