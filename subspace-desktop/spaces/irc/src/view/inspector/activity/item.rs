//! One item in full: a model step with its settled tokens and cost, a
//! tool call with its input, progress, output and errors, or reply and
//! reasoning text.

use gpui_kit::{AnyElement, Context};
use serde_json::Value;
use subspace_client_fleet::activity::{
    ActivityContent, ActivityError, ActivityItem, StepItem, TextItem, ToolItem,
};

use super::parts::{block, caption, card, labelled};
use super::text::visible;
use crate::Irc;

/// One item in full.
pub(super) fn item(id: &str, item: &ActivityItem, cx: &Context<Irc>) -> AnyElement {
    match item {
        ActivityItem::Step(step) => self::step(id, step, cx),
        ActivityItem::Text(text) => words(id, "Reply", text, cx),
        ActivityItem::Reasoning(text) => words(id, "Reasoning", text, cx),
        ActivityItem::Tool(tool) => self::tool(id, tool, cx),
    }
}

pub(super) fn words(id: &str, label: &str, text: &TextItem, cx: &Context<Irc>) -> AnyElement {
    let label = match text.status.as_str() {
        "completed" => label.to_owned(),
        status => format!("{label} · {}", visible(status)),
    };
    card(
        id,
        &label,
        vec![block(&format!("{id}:text"), &text.text, cx)],
        cx,
    )
}

/// A model step: its model, how it finished, its settled tokens and cost,
/// and its error.
pub(super) fn step(id: &str, step: &StepItem, cx: &Context<Irc>) -> AnyElement {
    let model = step.model.as_ref().map(|model| match &model.variant {
        Some(variant) => format!("{}/{} ({variant})", model.provider, model.id),
        None => format!("{}/{}", model.provider, model.id),
    });
    let label = [
        Some(format!("Step · {}", step.status)),
        model,
        step.finish.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let mut parts = vec![caption(&format!("{id}:use"), &step_use(step), cx)];
    parts.extend(step.error.as_ref().map(|error| failure(id, error, cx)));
    card(id, &visible(&label), parts, cx)
}

/// `10 in · 5 out · 2 reasoning · 1 cache read · 0 cache write · $0.2500`,
/// or that the step has not settled.
pub(super) fn step_use(step: &StepItem) -> String {
    let Some(tokens) = step.tokens else {
        return "Not settled yet".to_owned();
    };
    let cost = step
        .cost
        .map_or_else(|| "no price".to_owned(), |cost| format!("${cost:.4}"));
    format!(
        "{} in · {} out · {} reasoning · {} cache read · {} cache write · {cost}",
        tokens.input, tokens.output, tokens.reasoning, tokens.cache_read, tokens.cache_write
    )
}

/// A tool call: its input, progress, output and error, each whole.
pub(super) fn tool(id: &str, tool: &ToolItem, cx: &Context<Irc>) -> AnyElement {
    let name = tool.name.as_deref().unwrap_or("unnamed tool");
    let label = visible(&format!("Tool · {name} · {}", tool.status));
    let mut parts = match &tool.input {
        Some(input) => value_blocks(&format!("{id}:input"), "Input", input, cx),
        None if !tool.raw_input.is_empty() => {
            vec![labelled(
                &format!("{id}:raw"),
                "Input as streamed",
                &tool.raw_input,
                cx,
            )]
        }
        None => Vec::new(),
    };
    let progress = if matches!(tool.status.as_str(), "input" | "running") {
        "Progress"
    } else {
        "Metadata"
    };
    if let Some(metadata) = &tool.metadata {
        parts.extend(value_blocks(&format!("{id}:meta"), progress, metadata, cx));
    }
    for (index, content) in tool.content.iter().flatten().enumerate() {
        parts.extend(output(&format!("{id}:out:{index}"), content, cx));
    }
    parts.extend(tool.error.as_ref().map(|error| failure(id, error, cx)));
    card(id, &label, parts, cx)
}

pub(super) fn output(id: &str, content: &ActivityContent, cx: &Context<Irc>) -> Option<AnyElement> {
    match content {
        ActivityContent::Text { text } => Some(labelled(id, "Output", text, cx)),
        ActivityContent::File { uri, mime, name } => {
            let label = match name {
                Some(name) => format!("File · {name} · {mime}"),
                None => format!("File · {mime}"),
            };
            Some(labelled(id, &visible(&label), uri, cx))
        }
        ActivityContent::Other => None,
    }
}

/// A JSON value whole: an object field by field, a string field as its
/// text, anything else as indented JSON.
pub(super) fn value_blocks(
    id: &str,
    label: &str,
    value: &Value,
    cx: &Context<Irc>,
) -> Vec<AnyElement> {
    match value {
        Value::Object(fields) if !fields.is_empty() => fields
            .iter()
            .map(|(key, value)| {
                let label = visible(&format!("{label} · {key}"));
                labelled(&format!("{id}:{key}"), &label, &json_text(value), cx)
            })
            .collect(),
        value => vec![labelled(id, label, &json_text(value), cx)],
    }
}

pub(super) fn json_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        value => serde_json::to_string_pretty(value).unwrap_or_default(),
    }
}

/// A provider failure: its type, message, status and response, whole.
pub(super) fn failure(id: &str, error: &ActivityError, cx: &Context<Irc>) -> AnyElement {
    labelled(&format!("{id}:error"), "Error", &error_text(error), cx)
}

pub(super) fn error_text(error: &ActivityError) -> String {
    let mut text = format!("{}: {}", error.kind, error.message);
    if let Some(status) = error.status {
        text.push_str(&format!("\nStatus {status}"));
    }
    if let Some(wait) = error.retry_after_ms {
        text.push_str(&format!("\nRetry after {wait} ms"));
    }
    if let Some(body) = &error.response_body {
        text.push_str(&format!("\n{body}"));
    }
    text
}
