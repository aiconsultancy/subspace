//! Cloud-task creation with one end-to-end deadline and a final dispatch gate.

use serde_json::{Value, json};
use subspace_core::copilot_agent_tasks::Submission;
use subspace_core::http::{Error, TIMEOUT, agent, status};

use super::{
    AGENT_API_VERSION, FALLBACK_API_VERSION, StartTask, Task, TaskScope, to_task, validate_start,
    validate_task_id,
};
use crate::{Client, transport};

pub(super) fn start(
    client: &Client,
    input: &StartTask,
    scope: &TaskScope,
    submission: &Submission,
) -> Result<Task, Error> {
    let repo = validate_start(input, scope)?;
    submission
        .remaining()
        .map_err(|error| Error::Network(error.to_string()))?;
    let login = client.work_login()?;
    submission
        .remaining()
        .map_err(|error| Error::Network(error.to_string()))?;
    let base_ref = match &input.base_ref {
        Some(value) => Some(value.clone()),
        None => client.default_branch(&login, &repo).ok(),
    };
    let mut body = json!({"prompt":input.prompt,"create_pull_request":input.create_pull_request});
    if let Some(model) = &input.model {
        body["model"] = json!(model);
    }
    if let Some(base) = base_ref {
        body["base_ref"] = json!(base);
    }
    let raw = post(
        client,
        &login,
        &format!("/agents/repos/{repo}/tasks"),
        body,
        submission,
    )?;
    let id = raw.get("id").and_then(Value::as_str).unwrap_or_default();
    validate_task_id(id).map_err(|_| transport::unexpected("task creation response"))?;
    let pulls = client.resolve_pull_artifacts(&login, &repo, std::slice::from_ref(&raw));
    Ok(to_task(&repo, &raw, input.subject.clone(), &pulls))
}

pub(super) fn post(
    client: &Client,
    login: &str,
    path: &str,
    body: Value,
    submission: &Submission,
) -> Result<Value, Error> {
    let url = format!("{}{}", client.api_url, path);
    let body = body.to_string();
    let send = |version| {
        let credential = client
            .credentials
            .github(Some(login))
            .map_err(|error| Error::Status {
                status: status::BAD_GATEWAY,
                message: error.to_string(),
            })?;
        // Credentials may block. Admission must be after them, immediately
        // before transport dispatch, and must serialize with UI expiration.
        let remaining = submission
            .begin_dispatch()
            .map_err(|error| Error::Network(error.to_string()))?;
        let request = agent()
            .post(&url)
            .config()
            .timeout_global(Some(remaining.min(TIMEOUT)))
            .build();
        let result = transport::post_rest_json_request(request, &credential, login, &body, version);
        if matches!(
            result,
            Err(Error::Status {
                status: status::UNAUTHORIZED,
                ..
            })
        ) {
            client.credentials.invalidate(&credential);
        }
        result
    };
    let (_, text) = match send(AGENT_API_VERSION) {
        Err(error) if transport::is_api_version_rejection(&error) => send(FALLBACK_API_VERSION),
        result => result,
    }?;
    serde_json::from_str(&text).map_err(|_| transport::unexpected("response"))
}

#[cfg(test)]
mod tests;
