//! Planning projects, project updates and versioned milestone writes.
//! These projects are separate from the issue teams returned by NETT's issue client.

use std::collections::HashSet;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use ureq::http::StatusCode;

use crate::{Backend, Client, Error, PageRequest, model::Page, transport};

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
pub enum MilestoneWriteError {
    #[error("The milestone or project changed. Reload it before making another change.")]
    Conflict,
    #[error("You don’t have permission to change milestones in this project.")]
    Forbidden,
    #[error("The API refused this change. Check the milestone’s fields and issue status.")]
    Invalid,
    #[error("The save could not be confirmed. Reload and check the milestone before trying again.")]
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
    ) -> Result<(), MilestoneWriteError> {
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
    ) -> Result<(), MilestoneWriteError> {
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
    ) -> Result<(), MilestoneWriteError> {
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
            .map_err(|_| MilestoneWriteError::Unconfirmed)?;
        self.current()?;
        if response.status() == success {
            return Ok(());
        }
        if transport::is_access_challenge(&response) {
            return Err(Error::SignInNeeded.into());
        }
        Err(match response.status().as_u16() {
            400 | 422 => MilestoneWriteError::Invalid,
            401 => Error::SignInNeeded.into(),
            403 => MilestoneWriteError::Forbidden,
            404 => Error::NotFound.into(),
            409 => MilestoneWriteError::Conflict,
            _ => MilestoneWriteError::Unconfirmed,
        })
    }
}

fn change_body(milestone: &Milestone, change: MilestoneChange) -> (Value, StatusCode) {
    let version = milestone.version;
    let mut body = match change {
        MilestoneChange::Edit(fields) => edited_fields(milestone, fields),
        MilestoneChange::Complete(completed) => json!({ "completed": completed }),
        MilestoneChange::Move(Move::Up) => json!({ "move": "up" }),
        MilestoneChange::Move(Move::Down) => json!({ "move": "down" }),
        MilestoneChange::Delete => return (json!({ "version": version }), StatusCode::NO_CONTENT),
    };
    body["version"] = json!(version);
    (body, StatusCode::OK)
}

fn edited_fields(before: &Milestone, fields: MilestoneFields) -> Value {
    let mut body = json!({});
    if before.name != fields.name {
        body["name"] = json!(fields.name);
    }
    if before.description.as_deref().unwrap_or_default()
        != fields.description.as_deref().unwrap_or_default()
    {
        body["description"] = json!(fields.description);
    }
    if before.target_date != fields.target_date {
        body["targetDate"] = json!(fields.target_date);
    }
    body
}
