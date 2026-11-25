//! One-line summaries of a tool call's arguments and title, which every
//! engine's transcript shows the same way.

use serde_json::Value;

/// The longest argument summary a tool row shows before it is cut.
const DETAIL_LIMIT: usize = 120;
/// The argument rendered without its key, because it names what the call does.
const LEAD_ARGUMENT: &str = "action";

/// A scalar argument as display text. Objects and arrays are left out: a tool
/// row is one line, and their contents belong in the expanded result.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn truncate(text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    let kept: String = text.chars().take(limit).collect();
    format!("{kept}…")
}

/// A one-line argument summary: the `action` value alone, then every other
/// scalar as `key=value`, in the order the SDK sent them.
///
/// `{"action":"github_search","query":"is:open"}` reads as
/// `github_search query=is:open`.
pub(super) fn argument_summary(arguments: &Value) -> String {
    if arguments.get("canvasId").is_some()
        && let Some(id) = arguments.get("instanceId").and_then(Value::as_str)
    {
        return format!("canvasInstanceId={id}");
    }
    let Some(fields) = arguments.as_object() else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(lead) = fields.get(LEAD_ARGUMENT).and_then(scalar) {
        parts.push(lead);
    }
    for (key, value) in fields {
        if key == LEAD_ARGUMENT {
            continue;
        }
        if let Some(value) = scalar(value) {
            parts.push(format!("{key}={value}"));
        }
    }
    truncate(parts.join(" "), DETAIL_LIMIT)
}

/// One line of at most `limit` characters, with an ellipsis when it is cut.
pub(super) fn one_line(text: &str, limit: usize) -> String {
    truncate(text.split_whitespace().collect::<Vec<_>>().join(" "), limit)
}
