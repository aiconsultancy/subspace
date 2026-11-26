//! The message every engine sends for a turn: the text and its attached
//! references, refused when either carries credential-like data.

use super::{EventQueue, PROMPT_LIMIT, contains_secret};
use crate::ContextRef;
use std::collections::HashSet;

pub(super) fn prepare_prompt(
    text: &str,
    context: &[ContextRef],
    events: &EventQueue,
) -> Result<String, String> {
    let unsafe_content = events.has_secret(text)
        || context
            .iter()
            .any(|item| events.has_secret(&item.content) || events.has_secret(&item.label));
    reject_secret_context(unsafe_content)?;
    prompt(text, context)
}

pub(super) fn reject_secret_context(unsafe_content: bool) -> Result<(), String> {
    if unsafe_content {
        return Err(
            "Message or context contains credential-like data. Remove it before sending.".into(),
        );
    }
    Ok(())
}

pub(super) fn prompt(text: &str, context: &[ContextRef]) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("Enter a message before sending.".into());
    }
    let mut prompt = text.to_owned();
    let mut seen = HashSet::new();
    for item in context {
        if seen.insert(&item.key) {
            if contains_secret(&item.content) {
                return Err(
                    "Context contains credential-like data. Remove it before sending.".into(),
                );
            }
            prompt.push_str(crate::ATTACHMENT_MARKER);
            prompt.push_str(&item.label);
            prompt.push('\n');
            prompt.push_str(&item.content);
        }
        if prompt.len() > PROMPT_LIMIT {
            return Err("Message and context exceed 256 KiB.".into());
        }
    }
    if prompt.len() > PROMPT_LIMIT {
        return Err("Message and context exceed 256 KiB.".into());
    }
    Ok(prompt)
}
