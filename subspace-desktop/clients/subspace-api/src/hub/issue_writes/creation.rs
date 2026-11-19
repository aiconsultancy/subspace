use super::{CreateKey, Hub, IssueChanges, IssueSave, IssueSaveError, IssueWriteScope, lock};
use crate::{
    issue_writes::choices::all_pages,
    model::{IssueDetail, IssueListItem},
};
use std::collections::HashSet;

pub(in crate::hub) struct Attempt {
    scope: IssueWriteScope,
    project: String,
    changes: IssueChanges,
    before: HashSet<String>,
    phase: Phase,
}
enum Phase {
    Posting,
    Checking,
    Unclear(bool),
    Saved(Box<IssueDetail>),
}
impl Attempt {
    pub(super) fn busy(&self) -> bool {
        matches!(self.phase, Phase::Posting | Phase::Checking)
    }
}

// qual:allow(coupling, oi) reason: "Create orchestration records the pre-send identity set and enforces explicit recovery before another POST."
impl Hub {
    pub fn create_issue(
        &self,
        scope: &IssueWriteScope,
        project: &str,
        key: &CreateKey,
        changes: &IssueChanges,
    ) -> Result<IssueSave, IssueSaveError> {
        changes.validate(true)?;
        let project_key = self.writable_project(scope, project)?;
        if let Some(result) = self.begin_create(scope, project, *key, changes)? {
            return Ok(result);
        }
        let result = self.send_create(scope, &project_key, project, (*key, changes));
        self.finish_create(key, &result);
        self.require_issue_scope(scope)?;
        result
    }

    fn begin_create(
        &self,
        scope: &IssueWriteScope,
        project: &str,
        key: CreateKey,
        changes: &IssueChanges,
    ) -> Result<Option<IssueSave>, IssueSaveError> {
        let mut writes = lock(&self.0.issue_writes);
        if let Some(attempt) = writes.creates.get_mut(&key) {
            if attempt.scope != *scope || attempt.project != project || attempt.changes != *changes
            {
                return Err(IssueSaveError::Unavailable);
            }
            match &attempt.phase {
                Phase::Saved(issue) => return Ok(Some(IssueSave::Saved(issue.clone()))),
                Phase::Unclear(true) => attempt.phase = Phase::Posting,
                _ => return Err(IssueSaveError::Unconfirmed),
            }
        } else {
            writes.creates.insert(
                key,
                Attempt {
                    scope: scope.clone(),
                    project: project.into(),
                    changes: changes.clone(),
                    before: HashSet::new(),
                    phase: Phase::Posting,
                },
            );
        }
        Ok(None)
    }

    fn send_create(
        &self,
        scope: &IssueWriteScope,
        project_key: &str,
        project: &str,
        (key, changes): (CreateKey, &IssueChanges),
    ) -> Result<IssueSave, IssueSaveError> {
        let rows = self.scan_creates(project_key, changes.title().ok_or(IssueSaveError::Title)?)?;
        let before = rows.into_iter().map(|r| r.fields.id).collect();
        self.require_issue_scope(scope)?;
        if let Some(attempt) = lock(&self.0.issue_writes).creates.get_mut(&key) {
            attempt.before = before;
        }
        self.0.client.create_issue(project, changes)
    }

    fn finish_create(&self, key: &CreateKey, result: &Result<IssueSave, IssueSaveError>) {
        let mut writes = lock(&self.0.issue_writes);
        match result {
            Ok(IssueSave::Saved(issue)) => {
                if let Some(attempt) = writes.creates.get_mut(key) {
                    attempt.phase = Phase::Saved(issue.clone());
                }
            }
            Err(IssueSaveError::Unconfirmed) => {
                if let Some(attempt) = writes.creates.get_mut(key) {
                    attempt.phase = Phase::Unclear(false);
                }
            }
            Err(_) => {
                writes.creates.remove(key);
            }
        }
    }

    pub fn check_created_issue(
        &self,
        scope: &IssueWriteScope,
        key: &CreateKey,
    ) -> Result<Option<Box<IssueDetail>>, IssueSaveError> {
        self.require_issue_scope(scope)?;
        let (project, changes, before) = {
            let mut writes = lock(&self.0.issue_writes);
            let a = writes
                .creates
                .get_mut(key)
                .ok_or(IssueSaveError::Unavailable)?;
            if a.scope != *scope || !matches!(a.phase, Phase::Unclear(_)) {
                return Err(IssueSaveError::Unconfirmed);
            }
            a.phase = Phase::Checking;
            (a.project.clone(), a.changes.clone(), a.before.clone())
        };
        let result = self.find_created(
            scope,
            &project,
            changes.title().ok_or(IssueSaveError::Title)?,
            &before,
        );
        if let Some(a) = lock(&self.0.issue_writes).creates.get_mut(key) {
            a.phase = match &result {
                Ok(Some(issue)) => Phase::Saved(issue.clone()),
                Ok(None) => Phase::Unclear(true),
                Err(_) => Phase::Unclear(false),
            };
        }
        result
    }

    fn find_created(
        &self,
        scope: &IssueWriteScope,
        project: &str,
        title: &str,
        before: &HashSet<String>,
    ) -> Result<Option<Box<IssueDetail>>, IssueSaveError> {
        let key = self.writable_project(scope, project)?;
        let rows = self.scan_creates(&key, title)?;
        let mut new = rows.into_iter().filter(|r| {
            r.fields.title == title
                && r.fields.project_id == project
                && !before.contains(&r.fields.id)
        });
        let Some(row) = new.next() else {
            return Ok(None);
        };
        if new.next().is_some() {
            return Err(IssueSaveError::SeveralMatches);
        }
        let issue = self.latest_issue(scope, &row.fields.id)?;
        Ok((issue.fields.id == row.fields.id
            && issue.fields.title == title
            && issue.fields.project_id == project
            && issue
                .created_by
                .as_ref()
                .is_some_and(|p| p.user_id == scope.user)
            && !before.contains(&issue.fields.id))
        .then(|| Box::new(issue)))
    }

    fn scan_creates(
        &self,
        project: &str,
        title: &str,
    ) -> Result<Vec<IssueListItem>, IssueSaveError> {
        let rows = all_pages(|page| {
            let mut query = page.parameters(crate::query::MAX_LIST_PAGE_SIZE)?;
            query.extend([
                ("project".into(), project.into()),
                ("creator".into(), "me".into()),
                ("q".into(), title.into()),
                ("includeArchived".into(), "true".into()),
            ]);
            self.0.client.get(&["issues"], &query)
        })?;
        Ok(rows)
    }
}
