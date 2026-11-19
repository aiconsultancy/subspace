//! A live hint's re-read of the open issue.
use super::{Job, Shared, lock, milestone};
use crate::{
    IssueQuery, PageRequest,
    hub::{
        IssueStatus, ReadPriority, State,
        worker::{
            Task,
            pages::{Extent, fill},
        },
    },
    model::Page,
};
use std::time::Instant;

/// A live hint's read: the new detail, with its sub-issue and relation
/// pages followed from their own fresh cursors as far as they were paged in,
/// replaces the shown one whole; the timeline head, pull requests and media
/// lists are read again after, and nothing shown is cleared on failure.
/// Collaboration writes keep the version, so an equal version is still
/// taken; only a reply older than what is held is dropped.
pub(in crate::hub::worker) fn recheck(shared: &Shared, job: &Job, identifier: &str) {
    let held = Held::of(&lock(&shared.state));
    let result = shared
        .client
        .issue(identifier)
        .and_then(|detail| repage(shared, identifier, detail, held));
    let mut state = lock(&shared.state);
    if let Ok(detail) = result {
        take_recheck(shared, job, &mut state, (identifier, held), detail);
    }
    land_recheck(shared, job, &mut state);
}

/// How many sub-issue and relation rows the open issue shows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(in crate::hub::worker) struct Held {
    children: usize,
    relations: usize,
}

impl Held {
    pub(in crate::hub::worker) fn of(state: &State) -> Self {
        let detail = state
            .snapshot
            .issue
            .as_ref()
            .and_then(|issue| issue.detail.as_ref());
        let rows = |page: Option<usize>| page.unwrap_or(0);
        Self {
            children: rows(detail.and_then(|d| d.children.as_ref().map(|p| p.items.len()))),
            relations: rows(detail.and_then(|d| d.relations.as_ref().map(|p| p.items.len()))),
        }
    }
}

/// Pages the fresh sub-issues and relations from their own cursors up to the
/// rows held before, a bounded number of later pages each.
fn repage(
    shared: &Shared,
    identifier: &str,
    mut detail: crate::model::IssueDetail,
    held: Held,
) -> Result<crate::model::IssueDetail, crate::Error> {
    let size = PageRequest::default().limit as usize;
    if let Some(children) = detail.children.as_mut() {
        let mut query = IssueQuery::new("");
        query.parent = Some(detail.fields.id.clone());
        query.order = Some("manual".into());
        let next = |cursor: &str| {
            let mut query = query.clone();
            query.page.cursor = Some(cursor.into());
            shared.client.issues(&query)
        };
        let extent = Extent {
            rows: held.children,
            size,
        };
        fill_page(children, extent, next, |row| &row.fields.id)?;
    }
    if let Some(relations) = detail.relations.as_mut() {
        let next = |cursor: &str| {
            let page = PageRequest {
                cursor: Some(cursor.into()),
                ..PageRequest::default()
            };
            shared.client.relations(identifier, &page)
        };
        let extent = Extent {
            rows: held.relations,
            size,
        };
        fill_page(relations, extent, next, |row| &row.issue.fields.id)?;
    }
    Ok(detail)
}

fn fill_page<T>(
    page: &mut Page<T>,
    extent: Extent,
    next: impl FnMut(&str) -> Result<Page<T>, crate::Error>,
    id: impl Fn(&T) -> &str,
) -> Result<(), crate::Error> {
    fill(&mut page.items, &mut page.next_cursor, extent, next, id)
}

fn take_recheck(
    shared: &Shared,
    job: &Job,
    state: &mut State,
    read: (&str, Held),
    detail: crate::model::IssueDetail,
) {
    let (identifier, held) = read;
    if !job.current(state) {
        return;
    }
    if Held::of(state) != held {
        // Paged further while this read was out: read again rather than
        // install a shorter page over rows the person asked for.
        state.live_reads.issue.owe();
        return;
    }
    let Some(issue) = state
        .snapshot
        .issue
        .as_mut()
        .filter(|issue| issue.identifier == identifier && issue.status == IssueStatus::Loaded)
    else {
        return;
    };
    if issue
        .detail
        .as_ref()
        .is_some_and(|held| held.fields.version > detail.fields.version)
    {
        return;
    }
    if issue
        .detail
        .as_ref()
        .is_none_or(|before| before.fields.milestone_id != detail.fields.milestone_id)
    {
        issue.milestone_name = None;
    }
    issue.detail = Some(detail);
    super::replaced_pages(issue);
    issue.updated_at = Some(Instant::now());
    milestone::queue(shared, state);
    state.refresh_media(identifier);
    for task in [
        Task::Refresh(identifier.into()),
        // Links and unlinks keep the issue's version: read the head again.
        Task::PullRequests(
            identifier.into(),
            PageRequest {
                limit: crate::hub::pull_requests::PAGE_SIZE,
                cursor: None,
            },
            0,
        ),
    ] {
        shared.enqueue(state, task, ReadPriority::Visible);
    }
    state.changed();
}

/// The read is back, whatever came of it: hints that arrived meanwhile owe
/// one trailing read.
fn land_recheck(shared: &Shared, job: &Job, state: &mut State) {
    if state
        .live_reads
        .issue
        .land((job.generation, job.issue_revision))
        && job.current(state)
    {
        crate::hub::live::reread_issue(shared, state);
    }
}
