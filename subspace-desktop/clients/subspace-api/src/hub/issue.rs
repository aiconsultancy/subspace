use super::{
    Hub, HubError, IssueSnapshot, IssueStatus, ReadPriority, Shared, State, lock, worker::Task,
};
use crate::{IssueQuery, PageRequest};

impl Hub {
    pub fn open_issue(&self, identifier: &str, profile_id: &str) -> Result<(), HubError> {
        crate::transport::validate_segments(std::iter::once(identifier))?;
        let mut state = lock(&self.0.state);
        state.running()?;
        state.issue_revision = state.issue_revision.wrapping_add(1);
        let status = if profile_id == crate::PROFILE_ID {
            IssueStatus::Loading
        } else {
            IssueStatus::Unavailable
        };
        state.snapshot.issue = Some(IssueSnapshot::new(identifier, profile_id, status));
        state.changed();
        if profile_id == crate::PROFILE_ID && state.snapshot.me.is_some() {
            self.0.enqueue(
                &mut state,
                Task::Issue(identifier.into()),
                ReadPriority::Visible,
            );
        }
        Ok(())
    }
    /// An explicit refresh: everything shown is read again, including the
    /// decision list from its first page.
    pub fn reload_issue(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        self.0.resume_nett();
        self.0.refresh_open_issue(&mut state);
        Ok(())
    }
    pub fn close_issue(&self) {
        let mut state = lock(&self.0.state);
        state.issue_revision = state.issue_revision.wrapping_add(1);
        state.snapshot.issue = None;
        state.changed();
    }
    pub fn load_more_subissues(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state.snapshot.issue.as_mut() else {
            return Ok(());
        };
        if issue.children_loading {
            return Ok(());
        }
        let Some(detail) = issue.detail.as_ref() else {
            return Ok(());
        };
        let Some(cursor) = detail
            .children
            .as_ref()
            .and_then(|page| page.next_cursor.clone())
        else {
            return Ok(());
        };
        let mut query = IssueQuery::new("");
        // Detail children cursors are scoped to the UUID, even when opened by key.
        query.parent = Some(detail.fields.id.clone());
        query.order = Some("manual".into());
        query.page.cursor = Some(cursor);
        issue.children_loading = true;
        issue.children_failed = false;
        let epoch = issue.epochs.children;
        state.changed();
        self.0.enqueue(
            &mut state,
            Task::Children(query, epoch),
            ReadPriority::LaterPage,
        );
        Ok(())
    }
    pub fn load_more_relations(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state.snapshot.issue.as_mut() else {
            return Ok(());
        };
        if issue.relations_loading {
            return Ok(());
        }
        let Some(cursor) = issue
            .detail
            .as_ref()
            .and_then(|detail| detail.relations.as_ref())
            .and_then(|page| page.next_cursor.clone())
        else {
            return Ok(());
        };
        let task = Task::Relations(
            issue.identifier.clone(),
            PageRequest {
                cursor: Some(cursor),
                ..PageRequest::default()
            },
            issue.epochs.relations,
        );
        issue.relations_loading = true;
        issue.relations_failed = false;
        state.changed();
        self.0.enqueue(&mut state, task, ReadPriority::LaterPage);
        Ok(())
    }
}

impl Shared {
    fn refresh_open_issue(&self, state: &mut State) {
        if let Some(issue) = state.snapshot.issue.as_mut() {
            issue.decisions.restart();
        }
        self.reload_open_issue(state);
    }

    pub(super) fn reload_open_issue(&self, state: &mut State) {
        let Some(issue) = state
            .snapshot
            .issue
            .as_mut()
            .filter(|issue| issue.profile_id == crate::PROFILE_ID)
        else {
            return;
        };
        // A refresh retires workers, but keeps this issue's loaded content
        // and decision read intents until we learn whether it is still available.
        issue.status = IssueStatus::Loading;
        issue.milestone_loading = false;
        issue.children_loading = false;
        issue.children_failed = false;
        issue.relations_loading = false;
        issue.relations_failed = false;
        issue.pull_requests.loading = false;
        issue.pull_requests.error = None;
        issue.todos.loading = false;
        issue.decisions.pause();
        issue.timeline.loading = false;
        issue.images = Default::default();
        // Uploads in progress are kept through a refresh of the same
        // opening: their transfers may already hold stored bytes.
        issue.files.clear_keeping_uploads();
        let identifier = issue.identifier.clone();
        state.issue_revision = state.issue_revision.wrapping_add(1);
        state.changed();
        if state.snapshot.me.is_some() {
            self.enqueue(state, Task::Issue(identifier), ReadPriority::Visible);
        }
    }
}
