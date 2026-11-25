//! How many bytes of the captain's activity the snapshot holds inline,
//! counted against the view's budget ([`super::load`]): every text,
//! input, output and value in UTF-8 bytes.

use serde_json::Value;
use subspace_client_fleet::activity::{
    ActivityContent, ActivityEntry, ActivityError, ActivityItem, AgentActivity,
};

/// What a number, a boolean or null counts for.
const SCALAR: u64 = 8;

/// About how many bytes the snapshot's inline content holds: the length
/// of every text, input, output and value in it.
pub(super) fn inline_bytes(activity: &AgentActivity) -> u64 {
    activity
        .turns
        .iter()
        .flat_map(|turn| &turn.items)
        .map(entry_bytes)
        .sum()
}

fn entry_bytes(entry: &ActivityEntry) -> u64 {
    let bytes = match entry {
        // Its identity only: its content is read on demand.
        ActivityEntry::Paged(_) => 0,
        ActivityEntry::Item(item) => item_bytes(item),
        ActivityEntry::Unknown(unknown) => value_bytes(&unknown.raw),
    };
    bytes.saturating_add(entry.base().id.len() as u64)
}

fn item_bytes(item: &ActivityItem) -> u64 {
    let text = |text: &str| text.len() as u64;
    let error = |error: Option<&ActivityError>| {
        error.map_or(0, |error| {
            text(&error.message) + error.response_body.as_deref().map_or(0, text)
        })
    };
    match item {
        ActivityItem::Text(words) | ActivityItem::Reasoning(words) => text(&words.text),
        ActivityItem::Step(step) => error(step.error.as_ref()),
        ActivityItem::Tool(tool) => {
            let content: u64 = tool
                .content
                .iter()
                .flatten()
                .map(|content| match content {
                    ActivityContent::Text { text: words } => text(words),
                    ActivityContent::File { uri, .. } => text(uri),
                    ActivityContent::Other(raw) => value_bytes(raw),
                })
                .sum();
            text(&tool.raw_input)
                + tool.input.as_ref().map_or(0, value_bytes)
                + tool.metadata.as_ref().map_or(0, value_bytes)
                + content
                + error(tool.error.as_ref())
        }
    }
}

/// About how many bytes a JSON value holds: its strings and keys, and
/// eight for any other value.
fn value_bytes(value: &Value) -> u64 {
    match value {
        Value::String(text) => text.len() as u64,
        Value::Array(values) => values.iter().map(value_bytes).sum(),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| key.len() as u64 + value_bytes(value))
            .sum(),
        _ => SCALAR,
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_fleet::activity::AgentActivity;

    use super::inline_bytes;

    /// Captain's ruling on round 3, point 3: inline text counts in UTF-8
    /// bytes, never characters, so text in other scripts is not undercounted.
    #[test]
    fn inline_text_counts_in_utf8_bytes() {
        let words = "ネコ🐈é".repeat(1000);
        let activity: AgentActivity = serde_json::from_value(serde_json::json!({
            "agent": {"id": "aurora-lyra", "role": "captain", "backend": "opencode",
                      "collected": true},
            "turns": [{
                "id": "t-1", "seq": 1, "status": "completed", "startedAt": 1,
                "updatedAt": 2, "endedAt": 2, "reconnects": 0,
                "usage": {"steps": 1, "tokens": {"input": 1, "output": 1, "reasoning": 0,
                                                  "cacheRead": 0, "cacheWrite": 0}, "cost": 0},
                "items": [{"id": "i1", "seq": 1, "startedAt": 1, "updatedAt": 2,
                           "kind": "text", "status": "completed", "text": words},
                          {"id": "i2", "seq": 1, "startedAt": 1, "updatedAt": 2,
                           "kind": "patch", "body": words}]
            }],
            "cursor": {"generation": "g1", "seq": 1}
        }))
        .unwrap();
        let bytes = inline_bytes(&activity);
        assert_eq!(words.chars().count(), 4000);
        assert_eq!(words.len(), 12_000);
        // Both texts in bytes, with the ids and the unknown item's fields.
        assert!(bytes >= 2 * 12_000, "{bytes}");
        assert!(bytes < 2 * 12_000 + 200, "{bytes}");
    }
}
