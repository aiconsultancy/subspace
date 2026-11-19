//! `github_write`: comments, pull request updates, reviews and assignees in
//! the configured repositories, each target read before the approval.

use super::{GithubTools, configured_key};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use subspace_framework::agent_tools::schema::{Action, action, batch, body, text};
use subspace_framework::agent_tools::{Access, ToolSpec, WriteService};
use subspace_framework::client_github::model::{GhItemDetail, ItemKind, ReviewEvent};
use subspace_framework::client_github::validate;
use subspace_framework::core::config::Config;

const DESCRIPTION: &str = "Apply exactly 1..20 GitHub writes after one approval for this call. All operations are validated and their targets read before approval; denial means no writes. Supports comments on issues and PRs, PR title/body/baseRefName/state updates, APPROVE/REQUEST_CHANGES/COMMENT reviews and assignee changes, in the configured repositories. Operations run once in order; results show partial successes/errors. Never automatically retry an ambiguous write. No merge, delete, credentials or arbitrary HTTP operations.";

const MAX_PR_FIELDS: usize = 4;
const MAX_TITLE: usize = 255;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Write {
    #[serde(rename = "github_comment")]
    Comment { key: String, body: String },
    #[serde(rename = "github_update_pr")]
    UpdatePr {
        key: String,
        fields: Map<String, Value>,
    },
    #[serde(rename = "github_review")]
    Review {
        key: String,
        event: ReviewEvent,
        #[serde(default)]
        body: String,
    },
    #[serde(rename = "github_assignees")]
    Assignees {
        key: String,
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
    },
}

impl Write {
    fn key(&self) -> &str {
        match self {
            Self::Comment { key, .. }
            | Self::UpdatePr { key, .. }
            | Self::Review { key, .. }
            | Self::Assignees { key, .. } => key,
        }
    }
}

impl WriteService for GithubTools {
    type Operation = Write;
    /// The work identity's login.
    type Prepared = String;

    fn validate(&self, operations: &[Write]) -> Result<(), String> {
        operations
            .iter()
            .try_for_each(|operation| validate_operation(&self.config, operation))
    }

    fn preflight(&self, operations: &[Write]) -> Result<String, String> {
        let login = self.login()?;
        operations
            .iter()
            .try_for_each(|operation| self.preflight_target(operation, &login))?;
        Ok(login)
    }

    fn describe(&self, _operations: &[Write], _login: &Self::Prepared) -> String {
        String::new()
    }

    fn write(
        &self,
        _index: usize,
        operation: &Write,
        login: &Self::Prepared,
    ) -> Result<Value, String> {
        let key = operation.key();
        let written = match operation {
            Write::Assignees { add, remove, .. } => {
                return self
                    .client
                    .set_assignees(login, key, add, remove)
                    .map(|detail| json!({"key":key,"assignees":assignee_logins(&detail)}))
                    .map_err(|error| error.to_string());
            }
            Write::Comment { body, .. } => {
                self.client
                    .agent_write(login, key, "comment", json!({ "body": body }))
            }
            Write::UpdatePr { fields, .. } => {
                self.client
                    .agent_write(login, key, "update_pr", json!(fields))
            }
            Write::Review { event, body, .. } => self.client.agent_write(
                login,
                key,
                "review",
                json!({"event":event.as_str(),"body":body}),
            ),
        };
        written.map_err(|error| error.to_string())
    }
}

impl GithubTools {
    fn preflight_target(&self, operation: &Write, login: &str) -> Result<(), String> {
        let key = operation.key();
        let item = self
            .client
            .item(login, key)
            .map_err(|error| error.to_string())?;
        validate_target(operation, item.item.kind, key)
    }
}

pub(super) fn assignee_logins(detail: &GhItemDetail) -> Vec<String> {
    detail
        .item
        .assignees
        .iter()
        .map(|user| user.login.clone())
        .collect()
}

/// Comments and assignees apply to any item; the rest only to a PR.
pub(super) fn validate_target(operation: &Write, kind: ItemKind, key: &str) -> Result<(), String> {
    let any_item = matches!(operation, Write::Comment { .. } | Write::Assignees { .. });
    if !any_item && kind != ItemKind::Pr {
        Err(format!("Not a pull request: {key}"))
    } else {
        Ok(())
    }
}

pub(super) fn validate_operation(config: &Config, operation: &Write) -> Result<(), String> {
    configured_key(config, operation.key())?;
    match operation {
        Write::Comment { body, .. } => validate::check_body(body, true)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Write::Review { event, body, .. } => validate::check_review_body(*event, body)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Write::UpdatePr { fields, .. } => validate_pr(fields),
        Write::Assignees { add, remove, .. } => validate_assignees(add, remove),
    }
}

fn validate_assignees(add: &[String], remove: &[String]) -> Result<(), String> {
    let add = validate::check_logins("add", add).map_err(|error| error.to_string())?;
    let remove = validate::check_logins("remove", remove).map_err(|error| error.to_string())?;
    if add.is_empty() && remove.is_empty() {
        Err("add or remove must name at least one login.".into())
    } else {
        Ok(())
    }
}

fn validate_pr(fields: &Map<String, Value>) -> Result<(), String> {
    if fields.is_empty() || fields.len() > MAX_PR_FIELDS {
        return Err("PR update requires 1..4 fields.".into());
    }
    fields
        .iter()
        .try_for_each(|(field, value)| validate_pr_field(field, value))
}

fn validate_pr_field(field: &str, value: &Value) -> Result<(), String> {
    let text = value.as_str().ok_or("PR field values must be strings.")?;
    match field {
        "title" if !text.trim().is_empty() && text.chars().count() <= MAX_TITLE => Ok(()),
        "body" => validate::check_body(text, false)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        "baseRefName" if subspace_framework::client_github::copilot_agents::validate_ref(text) => {
            Ok(())
        }
        "state" if matches!(text, "OPEN" | "CLOSED") => Ok(()),
        _ => Err(format!("Unsupported or invalid PR field: {field}")),
    }
}

pub(super) fn spec() -> ToolSpec {
    ToolSpec {
        name: "github_write",
        title: "Write GitHub",
        description: DESCRIPTION,
        access: Access::Write,
        arguments: batch(&actions()),
        approval: subspace_framework::agent_tools::ApprovalPolicy::PerCall,
    }
}

fn actions() -> Vec<Action> {
    vec![
        action(
            "github_comment",
            "Comment on an issue or PR.",
            vec![("key", text(), true), ("body", body(), true)],
        ),
        action(
            "github_update_pr",
            "Update a PR's title, body, base branch or state.",
            vec![("key", text(), true), ("fields", pr_fields(), true)],
        ),
        action(
            "github_review",
            "Review a PR.",
            vec![
                ("key", text(), true),
                (
                    "event",
                    json!({"type":"string","enum":["APPROVE","REQUEST_CHANGES","COMMENT"]}),
                    true,
                ),
                ("body", body(), false),
            ],
        ),
        action(
            "github_assignees",
            "Add or remove assignees on an issue or PR; name at least one login.",
            vec![
                ("key", text(), true),
                ("add", logins(), false),
                ("remove", logins(), false),
            ],
        ),
    ]
}

fn logins() -> Value {
    json!({"type":"array","maxItems":10,"items":{"type":"string"}})
}

fn pr_fields() -> Value {
    json!({"type":"object","additionalProperties":false,"minProperties":1,"properties":{"title":{"type":"string","minLength":1,"maxLength":255},"body":{"type":"string","maxLength":65536},"baseRefName":{"type":"string"},"state":{"enum":["OPEN","CLOSED"]}}})
}
