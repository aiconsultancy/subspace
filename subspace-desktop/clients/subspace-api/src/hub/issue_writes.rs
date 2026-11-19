//! Background-callable writes, fenced by hub generation and identity.
mod creation;
mod moves;
use super::{Hub, HubSnapshot, HubStatus, ScopeGeneration, lock};
use crate::issue_writes::{
    ChoiceSource, CreateKey, IssueChanges, IssueChoice, IssueSave, IssueSaveError,
};
use crate::model::IssueDetail;
pub(super) use creation::Attempt;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq)]
pub struct IssueWriteScope {
    generation: ScopeGeneration,
    user: String,
}
#[derive(Default)]
pub(super) struct Writes {
    creates: HashMap<CreateKey, Attempt>,
}

impl HubSnapshot {
    pub fn issue_write_scope(&self) -> Option<IssueWriteScope> {
        if self.status == HubStatus::Stopped {
            return None;
        }
        Some(IssueWriteScope {
            generation: self.generation,
            user: self.me.as_ref()?.user_id.clone(),
        })
    }
}

impl Hub {
    pub fn issue_write_scope(&self) -> Option<IssueWriteScope> {
        lock(&self.0.state).snapshot.issue_write_scope()
    }

    fn require_issue_scope(&self, scope: &IssueWriteScope) -> Result<(), IssueSaveError> {
        let state = lock(&self.0.state);
        if state.snapshot.issue_write_scope().as_ref() != Some(scope) {
            return Err(IssueSaveError::Unavailable);
        }
        if state.snapshot.status != HubStatus::Ready {
            return Err(IssueSaveError::NotSent);
        }
        Ok(())
    }

    fn writable_project(
        &self,
        scope: &IssueWriteScope,
        id: &str,
    ) -> Result<String, IssueSaveError> {
        self.require_issue_scope(scope)?;
        lock(&self.0.state)
            .snapshot
            .projects
            .iter()
            .find(|p| p.id == id && p.archived_at.is_none())
            .map(|p| p.key.clone())
            .ok_or(IssueSaveError::Unavailable)
    }

    pub fn issue_choices(
        &self,
        scope: &IssueWriteScope,
        source: &ChoiceSource,
    ) -> Result<Vec<IssueChoice>, IssueSaveError> {
        self.require_issue_scope(scope)?;
        let choices = self.0.client.issue_choices(source)?;
        self.require_issue_scope(scope)?;
        Ok(choices)
    }

    pub fn latest_issue(
        &self,
        scope: &IssueWriteScope,
        id: &str,
    ) -> Result<IssueDetail, IssueSaveError> {
        self.require_issue_scope(scope)?;
        let issue = self.0.client.issue(id)?;
        self.require_issue_scope(scope)?;
        Ok(issue)
    }

    pub fn update_issue(
        &self,
        scope: &IssueWriteScope,
        id: &str,
        version: u32,
        changes: &IssueChanges,
    ) -> Result<IssueSave, IssueSaveError> {
        let issue = self.latest_issue(scope, id)?;
        self.writable_project(scope, &issue.fields.project_id)?;
        if issue.fields.archived_at.is_some() {
            return Err(IssueSaveError::IssueArchived);
        }
        if issue.fields.version != i64::from(version) {
            return Err(IssueSaveError::Conflict);
        }
        if changes.is_tracker(issue.fields.kind == "tracker") && changes.tracker_properties() {
            return Err(IssueSaveError::Invalid);
        }
        self.require_issue_scope(scope)?;
        let result = self.0.client.update_issue(id, version, changes);
        self.require_issue_scope(scope)?;
        if matches!(result, Err(IssueSaveError::Conflict))
            && let Ok(latest) = self.latest_issue(scope, id)
            && latest.fields.archived_at.is_some()
        {
            return Err(IssueSaveError::IssueArchived);
        }
        result
    }

    pub fn forget_issue_create(&self, key: &CreateKey) {
        let mut writes = lock(&self.0.issue_writes);
        if writes.creates.get(key).is_none_or(|a| !a.busy()) {
            writes.creates.remove(key);
        }
    }
}
