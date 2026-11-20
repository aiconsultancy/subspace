use crate::hub::{Hub, IssueWriteScope};
use crate::issue_moves::{MovePreview, MoveResult};
use crate::issue_writes::IssueSaveError;

impl Hub {
    pub fn preview_issue_move(
        &self,
        scope: &IssueWriteScope,
        id: &str,
        destination: &str,
    ) -> Result<MovePreview, IssueSaveError> {
        let issue = self.latest_issue(scope, id)?;
        self.writable_project(scope, &issue.fields.project_id)?;
        self.writable_project(scope, destination)?;
        let version = issue
            .fields
            .version
            .try_into()
            .map_err(|_| IssueSaveError::Conflict)?;
        let preview = self.0.client.preview_issue_move(id, destination, version)?;
        self.require_issue_scope(scope)?;
        Ok(preview)
    }
    pub fn move_issue(
        &self,
        scope: &IssueWriteScope,
        preview: &MovePreview,
    ) -> Result<MoveResult, IssueSaveError> {
        self.require_issue_scope(scope)?;
        self.writable_project(scope, &preview.source.id)?;
        self.writable_project(scope, &preview.destination.id)?;
        let result = self.0.client.move_issue(preview);
        self.require_issue_scope(scope)?;
        result
    }
    /// Checks a NETT move whose confirmation went unanswered: replaying the
    /// identical confirmation when NETT records moves and it was sent within
    /// the last 24 hours, otherwise reading every previewed issue again
    /// without sending the move.
    pub fn verify_issue_move(
        &self,
        scope: &IssueWriteScope,
        preview: &MovePreview,
    ) -> Result<MoveResult, IssueSaveError> {
        self.require_issue_scope(scope)?;
        let result = self.0.client.verify_issue_move(preview);
        self.require_issue_scope(scope)?;
        result
    }
}
