//! Versioned project moves: preview first, then one explicitly confirmed write.
use crate::issue_writes::IssueSaveError;
use crate::{Client, model::IssueDetail, transport};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
const MAX_MOVE_ISSUES: usize = 500;
const TOKEN_HEX_LEN: usize = 64;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MoveProject {
    pub id: String,
    pub key: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MoveItem {
    pub id: String,
    pub version: u32,
    pub old_identifier: String,
    pub new_identifier: String,
    pub state_id: String,
    pub state_name: String,
    pub cycle_cleared: bool,
    pub milestone_cleared: bool,
    pub estimate_cleared: bool,
    pub parent_detached: bool,
    pub archived: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MovePreview {
    pub token: String,
    pub source: MoveProject,
    pub destination: MoveProject,
    pub issues: Vec<MoveItem>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MoveResult {
    pub issue: IssueDetail,
    pub moved: Vec<MoveItem>,
}

// qual:allow(coupling, oi) reason: "The bounded HTTP seam owns single-attempt writes, access challenges and validated responses."
impl Client {
    pub fn preview_issue_move(
        &self,
        id: &str,
        destination: &str,
        version: u32,
    ) -> Result<MovePreview, IssueSaveError> {
        let preview: MovePreview = self.move_request(
            id,
            "move-preview",
            json!({"destinationProjectId":destination,"version":version}),
        )?;
        if preview.issues.is_empty()
            || preview.issues.len() > MAX_MOVE_ISSUES
            || preview.token.len() != TOKEN_HEX_LEN
            || preview.issues[0].id != id
            || preview.issues[0].version != version
            || preview.destination.id != destination
        {
            return Err(IssueSaveError::Unavailable);
        }
        Ok(preview)
    }
    pub fn move_issue(&self, preview: &MovePreview) -> Result<MoveResult, IssueSaveError> {
        let root = preview.issues.first().ok_or(IssueSaveError::Invalid)?;
        let result: MoveResult = self.move_request(&root.id, "move", json!({"destinationProjectId":preview.destination.id,"version":root.version,"token":preview.token}))?;
        if result.issue.fields.id != root.id
            || result.issue.fields.identifier != root.new_identifier
            || result.issue.fields.project_id != preview.destination.id
            || result.moved.len() != preview.issues.len()
            || !result
                .moved
                .iter()
                .zip(&preview.issues)
                .all(|(a, b)| a.id == b.id && a.new_identifier == b.new_identifier)
        {
            return Err(IssueSaveError::Unconfirmed);
        }
        Ok(result)
    }
    fn move_request<T: DeserializeOwned>(
        &self,
        id: &str,
        action: &str,
        body: Value,
    ) -> Result<T, IssueSaveError> {
        let url = transport::endpoint(&self.base, &["issues", id, action], &[])
            .map_err(|_| IssueSaveError::NotSent)?;
        let response = transport::post_json(self, &url, &body, transport::READ_TIMEOUT)
            .map_err(|_| IssueSaveError::Unconfirmed)?;
        if transport::is_access_challenge(&response)
            || response.status() == ureq::http::StatusCode::UNAUTHORIZED
        {
            return Err(IssueSaveError::SignIn);
        }
        match response.status().as_u16() {
            200 => transport::read_json(response).map_err(|_| IssueSaveError::Unconfirmed),
            400 | 422 => Err(IssueSaveError::Invalid),
            403 => Err(IssueSaveError::Forbidden),
            404 => Err(IssueSaveError::Unavailable),
            409 => Err(IssueSaveError::Conflict),
            429 => Err(IssueSaveError::RateLimited),
            _ => Err(IssueSaveError::Unconfirmed),
        }
    }
}
