//! The decisions linked to an issue, independent of its activity timeline.
use super::worker::{Job, Task, detail::merge_page, pages::extend_unique};
use super::{
    Hub, HubError, IssueSnapshot, IssueStatus, ReadError, ReadPriority, Shared, State, lock,
};
use crate::{
    DecisionQuery, Error, PageRequest,
    model::{DecisionListItem, Page},
};

const PAGE_SIZE: u32 = 50;

#[cfg(test)]
mod reload_tests;

#[derive(Clone, Default)]
pub(super) struct IssueDecisions {
    page: Option<Page<DecisionListItem>>,
    pending: Option<Read>,
    error: Option<ReadError>,
    unavailable: bool,
    opened: bool,
    pages: usize,
    epoch: u64,
    retry: Option<Read>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Read {
    First,
    More,
    Refresh(usize),
}

pub(super) struct DecisionsRequest {
    identifier: String,
    page: PageRequest,
    epoch: u64,
    depth: usize,
    read: Read,
}

impl IssueSnapshot {
    // Cached detail remains interactive during a reload and after a network failure.
    pub(super) fn can_read_decisions(&self) -> bool {
        self.status == IssueStatus::Loaded
            || (self.detail.is_some()
                && matches!(
                    self.status,
                    IssueStatus::Loading | IssueStatus::Error(ReadError::Network)
                ))
    }

    pub fn decisions(&self) -> Option<&Page<DecisionListItem>> {
        self.decisions.page.as_ref()
    }

    pub fn decisions_loading(&self) -> bool {
        self.decisions.pending.is_some() && self.status != IssueStatus::Error(ReadError::Network)
    }

    /// An offline continuation can reconnect, unless a different read is waiting to resume.
    pub fn can_load_more_decisions(&self) -> bool {
        self.can_read_decisions()
            && self
                .decisions
                .can_load_more(self.status == IssueStatus::Error(ReadError::Network))
    }

    pub fn decisions_failed(&self) -> Option<&ReadError> {
        self.decisions.error.as_ref()
    }

    pub fn decisions_refresh_failed(&self) -> bool {
        matches!(self.decisions.retry, Some(Read::Refresh(_)))
    }

    pub fn decisions_unavailable(&self) -> bool {
        self.decisions.unavailable
    }
}

impl Hub {
    pub fn activate_decisions(&self) -> Result<(), HubError> {
        self.request_decisions(Some(Read::First))
    }

    pub fn load_more_decisions(&self) -> Result<(), HubError> {
        self.request_decisions(Some(Read::More))
    }

    pub fn retry_decisions(&self) -> Result<(), HubError> {
        self.request_decisions(None)
    }

    fn request_decisions(&self, read: Option<Read>) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state
            .snapshot
            .issue
            .as_mut()
            .filter(|i| i.can_read_decisions())
        else {
            return Ok(());
        };
        let offline = issue.status == IssueStatus::Error(ReadError::Network);
        if issue.decisions.accept(read, offline) {
            if offline && !matches!(read, Some(Read::First)) {
                self.0.reload_open_issue(&mut state);
            } else {
                state.changed();
                dispatch_pending(&self.0, &mut state);
            }
        }
        Ok(())
    }
}

pub(super) fn resume(shared: &Shared, state: &mut State) {
    if state
        .snapshot
        .issue
        .as_ref()
        .is_some_and(|issue| issue.decisions.pending.is_some())
    {
        dispatch_pending(shared, state);
    } else {
        refresh(shared, state);
    }
}

pub(super) fn refresh(shared: &Shared, state: &mut State) {
    if let Some(issue) = state.snapshot.issue.as_mut()
        && issue
            .decisions
            .stage(Some(issue.decisions.refresh_intent()))
    {
        state.changed();
        dispatch_pending(shared, state);
    }
}

pub(super) fn current(state: &State, request: &DecisionsRequest) -> bool {
    state.snapshot.issue.as_ref().is_some_and(|issue| {
        issue.status == IssueStatus::Loaded && issue.decisions.epoch == request.epoch
    })
}

fn dispatch_pending(shared: &Shared, state: &mut State) {
    if let Some(request) = state
        .snapshot
        .issue
        .as_ref()
        .filter(|issue| issue.status == IssueStatus::Loaded)
        .and_then(|issue| issue.decisions.request(&issue.identifier))
    {
        shared.enqueue(state, Task::Decisions(request), ReadPriority::Visible);
    }
}

impl IssueDecisions {
    pub(super) fn pause(&mut self) {
        // Invalidate the worker, not the intent: resume against the same cached page.
        self.epoch = self.epoch.wrapping_add(1);
    }

    fn accept(&mut self, read: Option<Read>, offline: bool) -> bool {
        if read == Some(Read::First) && self.opened {
            return false;
        }
        if offline && self.pending.is_some() {
            // Retry resumes the accepted intent. Repeating Show more can
            // also reconnect without changing its continuation cursor.
            return read.is_none() || read == Some(Read::More) && self.can_load_more(true);
        }
        self.stage(
            read.or(self.retry)
                .or_else(|| offline.then(|| self.refresh_intent())),
        )
    }

    fn can_load_more(&self, offline: bool) -> bool {
        !self.unavailable
            && (self.pending.is_none() || offline && self.pending == Some(Read::More))
            && self.page.as_ref().is_some_and(|p| p.next_cursor.is_some())
    }

    fn refresh_intent(&self) -> Read {
        // Carry the requested extra page through replacement, reload and retry.
        Read::Refresh(match self.pending.or(self.retry) {
            Some(Read::More) => self.pages + 1,
            Some(Read::Refresh(depth)) => depth,
            _ => self.pages.max(1),
        })
    }

    fn stage(&mut self, read: Option<Read>) -> bool {
        let Some(read) = read else { return false };
        let idle = self.pending.is_none() && !self.unavailable;
        let admissible = match read {
            Read::First => idle && (!self.opened || self.retry == Some(Read::First)),
            Read::More => self.can_load_more(false),
            Read::Refresh(_) => self.opened,
        };
        if !admissible {
            return false;
        }
        self.opened = true;
        self.pending = Some(read);
        self.error = None;
        self.retry = None;
        self.epoch = self.epoch.wrapping_add(1);
        true
    }

    fn request(&self, identifier: &str) -> Option<DecisionsRequest> {
        let read = self.pending?;
        let cursor = match read {
            Read::More => Some(self.page.as_ref()?.next_cursor.clone()?),
            _ => None,
        };
        Some(DecisionsRequest {
            identifier: identifier.into(),
            page: PageRequest {
                limit: PAGE_SIZE,
                cursor,
            },
            epoch: self.epoch,
            depth: match read {
                Read::Refresh(depth) => depth,
                _ => 1,
            },
            read,
        })
    }

    fn install(&mut self, read: Read, result: Result<(Page<DecisionListItem>, usize), Error>) {
        self.pending = None;
        self.unavailable = matches!(result, Err(Error::NotFound | Error::Forbidden));
        match result {
            Ok((page, pages)) if matches!(read, Read::More) => {
                merge_page(&mut self.page, page, |row| &row.id);
                self.pages += pages;
            }
            Ok((page, pages)) => {
                self.page = Some(page);
                self.pages = pages;
            }
            Err(_) if self.unavailable => {
                self.page = None;
                self.pages = 0;
            }
            Err(error) => {
                self.error = Some(error.into());
                self.retry = Some(read);
            }
        }
    }
}

pub(super) fn load(shared: &Shared, job: &Job, request: &DecisionsRequest) {
    let result = fetch(shared, job, request);
    let mut state = lock(&shared.state);
    if job.current(&state)
        && let Some(issue) = state.snapshot.issue.as_mut()
    {
        issue.decisions.install(request.read, result);
        state.changed();
    }
}

/// A refresh follows its own cursor chain to the requested depth.
/// If any part fails, the existing rows stay intact.
fn fetch(
    shared: &Shared,
    job: &Job,
    request: &DecisionsRequest,
) -> Result<(Page<DecisionListItem>, usize), Error> {
    let query = DecisionQuery::for_issue(&request.identifier);
    let mut page = shared.client.decisions(&query, &request.page)?;
    let items = std::mem::take(&mut page.items);
    extend_unique(&mut page.items, items, |row| &row.id);
    let mut cursors = std::collections::HashSet::new();
    cursors.extend(request.page.cursor.clone());
    let mut pages = 1;
    while let Some(cursor) = page.next_cursor.as_ref() {
        if !cursors.insert(cursor.clone()) {
            return Err(Error::InvalidResponse);
        }
        if pages == request.depth || !job.current(&lock(&shared.state)) {
            break;
        }
        let next = PageRequest {
            limit: PAGE_SIZE,
            cursor: Some(cursor.clone()),
        };
        let incoming = shared.client.decisions(&query, &next)?;
        extend_unique(&mut page.items, incoming.items, |row| &row.id);
        page.next_cursor = incoming.next_cursor;
        pages += 1;
    }
    Ok((page, pages))
}
