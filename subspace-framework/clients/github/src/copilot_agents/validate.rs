//! Validation of the agent-task inputs before any request.

use subspace_core::http::Error;

use super::MAX_TASK_ID_CHARS;
use super::MODELS;
use super::{MAX_PROMPT_UTF16, MAX_REF_CHARS, MAX_SUBJECT_ID_CHARS, StartTask, Subject, TaskScope};
use crate::validate::{check_repo, invalid};

/// Task ids are opaque, so only length and charset can be checked.
pub fn validate_task_id(task_id: &str) -> Result<(), Error> {
    if task_id.is_empty()
        || task_id.len() > MAX_TASK_ID_CHARS
        || !task_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(invalid("task id is invalid."));
    }
    Ok(())
}

pub fn validate_subject(subject: &Subject) -> Result<(), Error> {
    if subject.id.is_empty()
        || subject.id.len() > MAX_SUBJECT_ID_CHARS
        || !subject
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
    {
        return Err(invalid("subject id must be 1-200 safe characters."));
    }
    Ok(())
}
fn ref_charset_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REF_CHARS
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b))
}

fn ref_edges_ok(value: &str) -> bool {
    !value.starts_with('/')
        && !value.starts_with('-')
        && !value.ends_with('/')
        && !value.ends_with('.')
}

fn ref_segments_ok(value: &str) -> bool {
    !value.contains("..")
        && !value.contains("//")
        && value
            .split('/')
            .all(|part| !part.starts_with('.') && !part.ends_with(".lock"))
}

pub fn validate_ref(value: &str) -> bool {
    ref_charset_ok(value) && ref_edges_ok(value) && ref_segments_ok(value)
}
pub fn validate_prompt(prompt: &str) -> Result<(), Error> {
    if prompt.trim().is_empty() {
        return Err(invalid("prompt must be a non-empty string."));
    }
    if prompt.encode_utf16().count() > MAX_PROMPT_UTF16 {
        return Err(invalid("prompt must be at most 60000 UTF-16 units."));
    }
    Ok(())
}
pub fn validate_start(input: &StartTask, scope: &TaskScope) -> Result<String, Error> {
    validate_prompt(&input.prompt)?;
    let (owner, name) = check_repo(&input.repo)?;
    let repo = scope
        .configured_repos
        .iter()
        .find(|r| r.eq_ignore_ascii_case(&format!("{owner}/{name}")))
        .cloned()
        .ok_or_else(|| invalid("repo is not allowed for cloud agent tasks."))?;
    if let Some(model) = &input.model
        && !MODELS.iter().flatten().any(|known| known == model)
    {
        return Err(invalid(
            "model is not in the GitHub cloud-agent model whitelist.",
        ));
    }
    if let Some(reference) = &input.base_ref
        && !validate_ref(reference)
    {
        return Err(invalid("base_ref must be a valid branch name."));
    }
    if let Some(subject) = &input.subject {
        validate_subject(subject)?;
    }
    Ok(repo)
}
