pub(super) mod milestone;
mod recheck;

pub(super) use recheck::recheck;

use super::{Job, Shared, lock};
use crate::{Error, IssueQuery, PageRequest, hub::IssueStatus};
use std::time::Instant;

pub(super) fn load(shared: &Shared, job: &Job, identifier: &str) {
    let sent = Instant::now();
    let result = shared.client.issue(identifier);
    // A NETT token refused before it expires is renewed, and the issue
    // read again, instead of reported.
    if let Err(error) = result
        && !crate::hub::renewal::stands(shared, job.generation, sent, error)
    {
        return;
    }
    if let Ok(detail) = &result {
        _ = super::connection::ensure_states(shared, job, &detail.fields.project_key);
    }
    publish(shared, job, identifier, result);
}

fn publish(
    shared: &Shared,
    job: &Job,
    identifier: &str,
    result: Result<crate::model::IssueDetail, Error>,
) {
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if let Some(issue) = state.snapshot.issue.as_mut() {
        match result {
            Ok(detail) => loaded(issue, detail),
            Err(error) => {
                if error != Error::Network {
                    issue.detail = None;
                    issue.milestone_name = None;
                    issue.milestone_loading = false;
                    issue.updated_at = None;
                    issue.timeline.reset();
                    issue.pull_requests = Default::default();
                    issue.todos = Default::default();
                    issue.decisions = Default::default();
                    issue.decision = Default::default();
                }
                issue.status = if error == Error::NotFound {
                    IssueStatus::Unavailable
                } else {
                    IssueStatus::Error(error.into())
                };
            }
        }
    }
    if state
        .snapshot
        .issue
        .as_ref()
        .is_some_and(|issue| issue.status == IssueStatus::Loaded)
    {
        shared.enqueue(
            &mut state,
            super::Task::PullRequests(
                identifier.into(),
                PageRequest {
                    limit: crate::hub::pull_requests::PAGE_SIZE,
                    cursor: None,
                },
                // A first page starts a new epoch; it is never fenced.
                0,
            ),
            crate::hub::ReadPriority::Visible,
        );
        milestone::queue(shared, &mut state);
        crate::hub::todos::queue(shared, &mut state);
        crate::hub::decisions::resume(shared, &mut state);
        crate::hub::decision::resume_pending(shared, &mut state);
    }
    if state.live_reads.issue_after_load.take() == Some((job.generation, job.issue_revision)) {
        crate::hub::live::reread_issue(shared, &mut state);
    }
    state.changed();
}

fn loaded(issue: &mut crate::hub::IssueSnapshot, detail: crate::model::IssueDetail) {
    if issue
        .detail
        .as_ref()
        .is_none_or(|before| before.fields.milestone_id != detail.fields.milestone_id)
    {
        issue.milestone_name = None;
    }
    issue.milestone_loading = detail.fields.milestone_id.is_some();
    issue.detail = Some(detail);
    replaced_pages(issue);
    issue.status = IssueStatus::Loaded;
    issue.updated_at = Some(Instant::now());
    issue.timeline.reset();
    issue.pull_requests.loading = true;
    issue.pull_requests.error = None;
}

/// The sub-issue and relation pages were replaced: continuations asked for
/// before are dropped when they land, so their loading and failure marks go.
pub(super) fn replaced_pages(issue: &mut crate::hub::IssueSnapshot) {
    issue.epochs.replaced();
    issue.children_loading = false;
    issue.children_failed = false;
    issue.relations_loading = false;
    issue.relations_failed = false;
}

/// Sub-issues past the first page, for the page installation `epoch`. A
/// reply or error for a replaced page touches nothing.
pub(super) fn children(shared: &Shared, job: &Job, query: &IssueQuery, epoch: u64) {
    let result = shared.client.issues(query);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if let Some(issue) = state
        .snapshot
        .issue
        .as_mut()
        .filter(|issue| issue.epochs.children == epoch)
    {
        issue.children_loading = false;
        match result {
            Ok(page) => {
                if let Some(detail) = issue.detail.as_mut() {
                    merge_page(&mut detail.children, page, |row| &row.fields.id);
                }
            }
            Err(_) => {
                issue.children_failed = true;
            }
        }
    }
    state.changed();
}

/// Relations past the first page, for the page installation `epoch`. A
/// reply or error for a replaced page touches nothing.
pub(super) fn relations(shared: &Shared, job: &Job, request: (&str, &PageRequest, u64)) {
    let (identifier, page_request, epoch) = request;
    let result = shared.client.relations(identifier, page_request);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if let Some(issue) = state
        .snapshot
        .issue
        .as_mut()
        .filter(|issue| issue.epochs.relations == epoch)
    {
        issue.relations_loading = false;
        match result {
            Ok(page) => {
                if let Some(detail) = issue.detail.as_mut() {
                    merge_page(&mut detail.relations, page, |row| &row.issue.fields.id);
                }
            }
            Err(_) => {
                issue.relations_failed = true;
            }
        }
    }
    state.changed();
}

pub(in crate::hub) fn merge_page<T>(
    target: &mut Option<crate::model::Page<T>>,
    incoming: crate::model::Page<T>,
    id: impl Fn(&T) -> &str,
) {
    if let Some(target) = target {
        super::pages::extend_unique(&mut target.items, incoming.items, id);
        target.next_cursor = incoming.next_cursor;
    } else {
        *target = Some(incoming);
    }
}

#[cfg(test)]
mod tests;
