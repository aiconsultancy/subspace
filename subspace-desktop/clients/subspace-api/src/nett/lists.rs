//! NETT issue lists, the desktop's to-dos: named lists on an issue, each
//! with checkable items. Writes carry the item's version, and a stale
//! version is a conflict rather than an overwrite.

use serde::Deserialize;
use serde_json::{Value, json};
use ureq::http::StatusCode;

use crate::issue_writes::IssueSaveError;
use crate::{Client, Error, transport};

/// One named list on an issue.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TodoList {
    pub id: String,
    pub name: String,
    pub version: u64,
    #[serde(default)]
    pub item_count: u32,
    #[serde(default)]
    pub completed_item_count: u32,
}

/// One item on a list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TodoItem {
    pub id: String,
    pub list_id: String,
    pub text: String,
    pub completed: bool,
    pub version: u64,
}

// qual:allow(coupling, oi) reason: "The list boundary composes organisation routes, versioned writes and response checks."
impl Client {
    /// The issue's lists, in NETT's order. Only NETT has them.
    pub fn issue_lists(&self, issue: &str) -> Result<Vec<TodoList>, Error> {
        let issue = self.lists_issue(issue)?;
        let rows = self.nett_all(&["issues", &issue, "lists"], &[])?;
        rows.into_iter()
            .filter(|row| row.get("removed").and_then(Value::as_bool) != Some(true))
            .map(|row| serde_json::from_value(row).map_err(|_| Error::InvalidResponse))
            .collect()
    }

    /// The items on one of the issue's lists.
    pub fn issue_list_items(&self, issue: &str, list: &str) -> Result<Vec<TodoItem>, Error> {
        let issue = self.lists_issue(issue)?;
        let rows = self.nett_all(&["issues", &issue, "lists", list, "items"], &[])?;
        rows.into_iter()
            .filter(|row| row.get("removed").and_then(Value::as_bool) != Some(true))
            .map(|row| serde_json::from_value(row).map_err(|_| Error::InvalidResponse))
            .collect()
    }

    /// Checks or unchecks an item at the version the person saw.
    pub fn set_issue_list_item_done(
        &self,
        issue: &str,
        item: &TodoItem,
        completed: bool,
    ) -> Result<TodoItem, IssueSaveError> {
        let issue = self.lists_issue(issue).map_err(IssueSaveError::from)?;
        let url = self
            .endpoint(
                &["issues", &issue, "lists", &item.list_id, "items", &item.id],
                &[],
            )
            .map_err(|_| IssueSaveError::NotSent)?;
        let body = json!({"version": item.version, "completed": completed});
        transport::prepare(&url);
        let mut request = self
            .agent
            .put(url.as_str())
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
            .map_err(|_| IssueSaveError::Unconfirmed)?;
        if transport::is_access_challenge(&response)
            || response.status() == StatusCode::UNAUTHORIZED
        {
            return Err(IssueSaveError::SignIn);
        }
        match response.status().as_u16() {
            200 => {
                let saved: Value =
                    transport::read_json(response).map_err(|_| IssueSaveError::Unconfirmed)?;
                let saved = saved.get("item").cloned().unwrap_or(Value::Null);
                let saved: TodoItem =
                    serde_json::from_value(saved).map_err(|_| IssueSaveError::Unconfirmed)?;
                if saved.id != item.id
                    || saved.list_id != item.list_id
                    || saved.completed != completed
                    || saved.version <= item.version
                {
                    return Err(IssueSaveError::Unconfirmed);
                }
                // A reply read after the account or organisation changed
                // belongs to no one.
                self.current().map_err(|_| IssueSaveError::SignIn)?;
                Ok(saved)
            }
            400 | 422 => Err(IssueSaveError::Invalid),
            403 => Err(IssueSaveError::Forbidden),
            404 => Err(IssueSaveError::Unavailable),
            409 => Err(IssueSaveError::Conflict),
            429 | 503 => Err(IssueSaveError::RateLimited),
            _ => Err(IssueSaveError::Unconfirmed),
        }
    }

    /// List routes take the issue's UUID; Subspace API has no lists.
    fn lists_issue(&self, issue: &str) -> Result<String, Error> {
        if self.backend != crate::Backend::Nett {
            return Err(Error::NotFound);
        }
        self.nett_issue_id(issue)
    }
}
