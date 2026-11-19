//! Independent, generation-fenced, memory-only association pages.
use super::{
    Hub, HubError, ReadPriority, Shared, lock,
    worker::{Job, Task},
};
use crate::{
    Error, PageRequest,
    model::{Page, PullRequest},
};

pub(super) const PAGE_SIZE: u32 = 20;

#[derive(Clone, Default)]
pub(super) struct PullRequests {
    pub page: Option<Page<PullRequest>>,
    pub loading: bool,
    pub error: Option<super::ReadError>,
    pub unavailable: bool,
    /// Bumped whenever a first page replaces what is shown; a continuation
    /// applies only to the epoch it was asked for.
    pub epoch: u64,
}

impl Hub {
    pub fn load_more_pull_requests(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state.snapshot.issue.as_mut() else {
            return Ok(());
        };
        if issue.detail.is_none() || issue.status != super::IssueStatus::Loaded {
            return Ok(());
        }
        if issue.pull_requests.loading || issue.pull_requests.unavailable {
            return Ok(());
        }
        let mut page = PageRequest {
            limit: PAGE_SIZE,
            cursor: None,
        };
        if let Some(loaded) = &issue.pull_requests.page {
            let Some(cursor) = &loaded.next_cursor else {
                return Ok(());
            };
            page.cursor = Some(cursor.clone());
        }
        let identifier = issue.identifier.clone();
        issue.pull_requests.loading = true;
        issue.pull_requests.error = None;
        let epoch = issue.pull_requests.epoch;
        state.changed();
        self.0.enqueue(
            &mut state,
            Task::PullRequests(identifier, page, epoch),
            ReadPriority::LaterPage,
        );
        Ok(())
    }
}

pub(super) fn load(shared: &Shared, job: &Job, request: (&str, &PageRequest, u64)) {
    let (identifier, page, epoch) = request;
    let result = shared.client.pull_requests(identifier, page);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if let Some(issue) = state.snapshot.issue.as_mut() {
        issue.pull_requests.apply(result, page, epoch);
    }
    state.changed();
}

impl PullRequests {
    /// A first page replaces what is shown as a new epoch. A later page,
    /// or its error, applies only to the epoch it was asked for: one asked
    /// for before a replacement touches nothing, even with the same cursor.
    fn apply(
        &mut self,
        result: Result<Page<PullRequest>, Error>,
        request: &PageRequest,
        epoch: u64,
    ) {
        let more = request.cursor.is_some();
        if more && epoch != self.epoch {
            return;
        }
        self.loading = false;
        self.error = None;
        match result {
            Ok(page) => {
                self.unavailable = false;
                if more {
                    super::worker::detail::merge_page(&mut self.page, page, |pr| &pr.id);
                } else {
                    self.page = Some(page);
                    self.epoch = self.epoch.wrapping_add(1);
                }
            }
            Err(Error::NotFound) => {
                self.page = None;
                self.unavailable = true;
                self.epoch = self.epoch.wrapping_add(1);
            }
            Err(error) => {
                self.error = Some(error.into());
            }
        }
    }
}

#[cfg(test)]
mod tests;
