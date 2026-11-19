//! Per-issue history follows the current navigation revision.
use super::{Hub, HubError, ReadPriority, lock, worker::Task};
use crate::PageRequest;
pub(super) const AUTOMATIC_PAGES: u8 = 4;

#[derive(Clone)]
pub(super) struct TimelineRequest {
    pub identifier: String,
    pub page: PageRequest,
    pub comment_goal: usize,
    pub remaining: u8,
}

impl Hub {
    pub fn activate_timeline(&self, comment_goal: usize) -> Result<(), HubError> {
        self.timeline(false, false, comment_goal)
    }
    pub fn load_more_timeline(&self, comment_goal: usize) -> Result<(), HubError> {
        self.timeline(true, false, comment_goal)
    }
    pub fn retry_timeline(&self, comment_goal: usize) -> Result<(), HubError> {
        self.timeline(false, true, comment_goal)
    }
    fn timeline(&self, more: bool, retry: bool, comment_goal: usize) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state
            .snapshot
            .issue
            .as_mut()
            .filter(|issue| issue.status == super::IssueStatus::Loaded)
        else {
            return Ok(());
        };
        let timeline = &mut issue.timeline;
        if timeline.loading || (!more && !retry && !timeline.idle()) {
            return Ok(());
        }
        let cursor = timeline
            .content
            .page
            .as_ref()
            .and_then(|page| page.next_cursor.clone());
        if timeline.content.page.is_some() && cursor.is_none() {
            return Ok(());
        }
        let request = TimelineRequest {
            identifier: issue.identifier.clone(),
            page: PageRequest {
                cursor,
                ..PageRequest::default()
            },
            comment_goal,
            remaining: AUTOMATIC_PAGES - 1,
        };
        timeline.loading = true;
        timeline.error = None;
        state.changed();
        self.0
            .enqueue(&mut state, Task::Timeline(request), ReadPriority::Visible);
        Ok(())
    }
    pub fn refresh_discussion(&self, issue: &str) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        self.0.refresh_comments(&mut state, issue);
        Ok(())
    }
}

impl super::Shared {
    pub(super) fn refresh_comments(&self, state: &mut super::State, identifier: &str) {
        if state
            .snapshot
            .issue
            .as_ref()
            .and_then(|issue| issue.detail.as_ref())
            .is_some_and(|detail| {
                detail.fields.id == identifier || detail.fields.identifier == identifier
            })
        {
            self.enqueue(
                state,
                Task::Refresh(identifier.into()),
                ReadPriority::Visible,
            );
        }
    }
}
