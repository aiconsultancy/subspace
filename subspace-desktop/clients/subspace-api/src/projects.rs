//! Planning projects, project updates and versioned milestone writes.
//! These projects are separate from the issue teams returned by NETT's issue client.

mod payloads;

use std::collections::HashSet;

use payloads::change_body;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use ureq::http::StatusCode;

use crate::{
    Backend, Client, Error, PageRequest,
    model::{IssueListItem, Page},
    transport,
};

const PAGE_SIZE: u32 = 50;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningProject {
    pub id: String,
    pub key: Option<String>,
    pub name: String,
    pub lead_name: Option<String>,
    pub status: String,
    pub health: Option<String>,
    pub priority: u8,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub description: Option<String>,
    pub archived: bool,
    pub initiative_id: Option<String>,
    pub initiative: Option<ProjectInitiative>,
    pub version: Option<u32>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFields {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<Option<String>>,
}

#[derive(Clone, Deserialize)]
pub struct ProjectInitiative {
    pub name: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub body: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Milestone {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<String>,
    pub status: MilestoneStatus,
    pub completion_source: Option<CompletionSource>,
    pub issue_progress: Option<IssueProgress>,
    pub version: u32,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MilestoneStatus {
    Unstarted,
    Next,
    Overdue,
    Done,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompletionSource {
    Manual,
    Issues,
    ManualAndIssues,
}

#[derive(Clone, Deserialize)]
pub struct IssueProgress {
    pub done: u32,
    pub total: u32,
    #[serde(default)]
    pub canceled: u32,
    #[serde(default)]
    pub duplicate: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MilestoneFields {
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<String>,
}

#[derive(Clone, Copy)]
pub enum Move {
    Up,
    Down,
}

pub enum MilestoneChange {
    Edit(MilestoneFields),
    Complete(bool),
    Move(Move),
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlanningWriteError {
    #[error("The milestone or project changed. Reload it before making another change.")]
    Conflict,
    #[error("You don’t have permission to make this change.")]
    Forbidden,
    #[error("The API refused this change. Check the fields and status.")]
    Invalid,
    #[error("The save could not be confirmed. Reload and check the record before trying again.")]
    Unconfirmed,
    #[error(transparent)]
    Read(#[from] Error),
}

// qual:allow(coupling, oi) reason: "The planning routes share the client’s existing account lease, pagination and HTTP transport."
impl Client {
    pub fn planning_projects(&self) -> Result<Vec<PlanningProject>, Error> {
        self.planning_pages(&["projects"], &[("includeArchived".into(), "true".into())])
    }

    pub fn planning_project(&self, id: &str) -> Result<PlanningProject, Error> {
        self.current()?;
        self.get(&["projects", id], &[])
    }

    pub fn project_milestones(&self, id: &str) -> Result<Vec<Milestone>, Error> {
        self.planning_pages(&["projects", id, "milestones"], &[])
    }

    pub fn milestone_issues(&self, id: &str) -> Result<Vec<IssueListItem>, Error> {
        transport::validate_segments(std::iter::once(id))?;
        let mut rows = self.planning_pages(&["issues"], &milestone_issue_query(id))?;
        retain_milestone(&mut rows, id);
        Ok(rows)
    }

    pub fn milestone_issue_preview(&self, id: &str) -> Result<Page<IssueListItem>, Error> {
        self.current()?;
        transport::validate_segments(std::iter::once(id))?;
        let mut query = milestone_issue_query(id).to_vec();
        query.push(("limit".into(), PAGE_SIZE.to_string()));
        let mut page: Page<IssueListItem> = self.get(&["issues"], &query)?;
        retain_milestone(&mut page.items, id);
        Ok(page)
    }

    pub fn change_project(
        &self,
        id: &str,
        fields: &ProjectFields,
    ) -> Result<(), PlanningWriteError> {
        self.current()?;
        let url = self.endpoint(&["projects", id], &[])?;
        let body = json!(fields);
        if body.as_object().is_some_and(|fields| fields.len() == 1) {
            return Ok(());
        }
        let response = transport::put_json(self, &url, &body, transport::READ_TIMEOUT)
            .map_err(|_| PlanningWriteError::Unconfirmed)?;
        self.current()?;
        write_result(&response, StatusCode::OK)
    }

    pub fn latest_project_update(&self, id: &str) -> Result<Option<ProjectUpdate>, Error> {
        self.current()?;
        let page: Page<ProjectUpdate> = self.get(
            &["projects", id, "updates"],
            &[("limit".into(), "1".into())],
        )?;
        Ok(page.items.into_iter().next())
    }

    fn planning_pages<T: DeserializeOwned>(
        &self,
        path: &[&str],
        extra: &[(String, String)],
    ) -> Result<Vec<T>, Error> {
        let mut request = PageRequest {
            limit: PAGE_SIZE,
            cursor: None,
        };
        let mut items = Vec::new();
        let mut cursors = HashSet::new();
        loop {
            self.current()?;
            let mut query = request.parameters(PAGE_SIZE)?;
            query.extend_from_slice(extra);
            let page: Page<T> = self.get(path, &query)?;
            items.extend(page.items);
            let Some(cursor) = page.next_cursor.filter(|cursor| !cursor.is_empty()) else {
                return Ok(items);
            };
            if !cursors.insert(cursor.clone()) {
                return Err(Error::InvalidResponse);
            }
            request.cursor = Some(cursor);
        }
    }

    pub fn add_milestone(
        &self,
        project: &str,
        fields: &MilestoneFields,
    ) -> Result<(), PlanningWriteError> {
        self.write_milestone(
            &["projects", project, "milestones"],
            &json!(fields),
            StatusCode::CREATED,
        )
    }

    pub fn change_milestone(
        &self,
        project: &str,
        milestone: &Milestone,
        change: MilestoneChange,
    ) -> Result<(), PlanningWriteError> {
        let (body, status) = change_body(milestone, change);
        if status == StatusCode::OK && body.as_object().is_some_and(|body| body.len() == 1) {
            return Ok(());
        }
        self.write_milestone(
            &["projects", project, "milestones", &milestone.id],
            &body,
            status,
        )
    }

    fn write_milestone(
        &self,
        path: &[&str],
        body: &Value,
        success: StatusCode,
    ) -> Result<(), PlanningWriteError> {
        self.current()?;
        let url = self.endpoint(path, &[])?;
        transport::prepare(&url);
        let mut request = match (success, self.backend) {
            (StatusCode::CREATED, _) => self.agent.post(url.as_str()),
            (StatusCode::NO_CONTENT, _) => self.agent.delete(url.as_str()).force_send_body(),
            (_, Backend::Nett) => self.agent.put(url.as_str()),
            _ => self.agent.patch(url.as_str()),
        }
        .header("User-Agent", transport::USER_AGENT)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json");
        if let Some(jwt) = &self.access_jwt {
            request = request.header("Authorization", format!("Bearer {}", jwt.expose()));
        }
        let response = request
            .config()
            .timeout_global(Some(transport::READ_TIMEOUT))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build()
            .send(body.to_string().as_bytes())
            .map_err(|_| PlanningWriteError::Unconfirmed)?;
        self.current()?;
        write_result(&response, success)
    }
}

fn milestone_issue_query(id: &str) -> [(String, String); 2] {
    [
        ("milestone".into(), id.into()),
        ("includeArchived".into(), "true".into()),
    ]
}

fn retain_milestone(rows: &mut Vec<IssueListItem>, id: &str) {
    rows.retain(|row| row.fields.milestone_id.as_deref() == Some(id));
}

fn write_result(
    response: &ureq::http::Response<ureq::Body>,
    success: StatusCode,
) -> Result<(), PlanningWriteError> {
    if response.status() == success {
        return Ok(());
    }
    if transport::is_access_challenge(response) {
        return Err(Error::SignInNeeded.into());
    }
    Err(match response.status().as_u16() {
        400 | 422 => PlanningWriteError::Invalid,
        401 => Error::SignInNeeded.into(),
        403 => PlanningWriteError::Forbidden,
        404 => Error::NotFound.into(),
        409 => PlanningWriteError::Conflict,
        _ => PlanningWriteError::Unconfirmed,
    })
}
