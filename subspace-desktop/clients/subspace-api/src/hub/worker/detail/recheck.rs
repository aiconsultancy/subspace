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
    let sent = Instant::now();
    let result = shared
        .client
        .issue(identifier)
        .and_then(|detail| repage(shared, identifier, detail, held));
    // NETT: access went, so what was shown goes too, unless a token refused
    // before it expired was renewed and the issue is read again.
    let refused = match result {
        Err(
            error @ (crate::Error::SignInNeeded | crate::Error::Forbidden | crate::Error::NotFound),
        ) if shared.client.service() == crate::Backend::Nett => {
            crate::hub::renewal::stands(shared, job.generation, sent, error).then_some(error)
        }
        _ => None,
    };
    let mut state = lock(&shared.state);
    match (result, refused) {
        (Ok(detail), _) => take_recheck(shared, job, &mut state, (identifier, held), detail),
        (Err(_), Some(error)) => retire(job, &mut state, identifier, error),
        _ => {}
    }
    land_recheck(shared, job, &mut state);
}

/// Clears an issue the person can no longer read, as a failed first read
/// would have. Its revision is retired too, so no read already out for it
/// can bring any of it back.
fn retire(job: &Job, state: &mut State, identifier: &str, error: crate::Error) {
    if !job.current(state) {
        return;
    }
    if !state
        .snapshot
        .issue
        .as_ref()
        .is_some_and(|issue| issue.identifier == identifier)
    {
        return;
    }
    state.issue_revision = state.issue_revision.wrapping_add(1);
    let Some(issue) = state.snapshot.issue.as_mut() else {
        return;
    };
    issue.detail = None;
    issue.milestone_name = None;
    issue.milestone_loading = false;
    issue.updated_at = None;
    issue.timeline.reset();
    issue.pull_requests = Default::default();
    issue.todos = Default::default();
    issue.decisions = Default::default();
    issue.decision = Default::default();
    issue.images = Default::default();
    issue.files.clear_keeping_uploads();
    issue.status = if error == crate::Error::NotFound {
        IssueStatus::Unavailable
    } else {
        IssueStatus::Error(error.into())
    };
    state.changed();
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
    crate::hub::todos::queue(shared, state);
    state.refresh_media(identifier);
    crate::hub::decisions::refresh(shared, state);
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
