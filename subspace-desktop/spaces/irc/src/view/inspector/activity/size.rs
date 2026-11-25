//! How many bytes of the captain's activity the view holds, counted
//! against its budget ([`super::load`], [`super::draw`]): the snapshot's
//! inline content, every text, input, output and value in UTF-8 bytes;
//! and what an item's body draws, each block its length, or one part when
//! it is long, an unknown item its pretty JSON.

use std::io;

use serde_json::Value;
use subspace_client_fleet::activity::{
    ActivityContent, ActivityEntry, ActivityError, ActivityItem, AgentActivity, ToolItem,
};

use super::item::error_text;
use super::parts::{LONG, PART};

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

/// What a block of `len` bytes draws: all of it, or one part when it
/// shows in parts.
pub(super) fn block_bytes(len: usize) -> u64 {
    if len > LONG { PART as u64 } else { len as u64 }
}

/// What the body of an item drawn whole holds: every block it draws. An
/// item too large to send inline draws nothing of its own; what is read
/// of it is counted by [`super::draw`].
pub(super) fn body_bytes(entry: &ActivityEntry) -> u64 {
    match entry {
        ActivityEntry::Item(item) => item_body(item),
        ActivityEntry::Unknown(unknown) => block_bytes(pretty_len(&unknown.raw)),
        ActivityEntry::Paged(_) => 0,
    }
}

fn item_body(item: &ActivityItem) -> u64 {
    let error = |error: Option<&ActivityError>| {
        error.map_or(0, |error| block_bytes(error_text(error).len()))
    };
    match item {
        ActivityItem::Text(words) | ActivityItem::Reasoning(words) => block_bytes(words.text.len()),
        ActivityItem::Step(step) => error(step.error.as_ref()),
        ActivityItem::Tool(tool) => tool_body(tool) + error(tool.error.as_ref()),
    }
}

/// A tool's input, as parsed and as streamed, metadata and output.
fn tool_body(tool: &ToolItem) -> u64 {
    let content: u64 = tool
        .content
        .iter()
        .flatten()
        .map(|content| match content {
            ActivityContent::Text { text } => block_bytes(text.len()),
            ActivityContent::File { uri, .. } => block_bytes(uri.len()),
            ActivityContent::Other(raw) => block_bytes(pretty_len(raw)),
        })
        .sum();
    tool.input.as_ref().map_or(0, fields_body)
        + block_bytes(tool.raw_input.len())
        + tool.metadata.as_ref().map_or(0, fields_body)
        + content
}

/// A value drawn field by field when it is an object with fields, as
/// [`super::item::value_blocks`] draws it; whole otherwise.
fn fields_body(value: &Value) -> u64 {
    let text = |value: &Value| match value {
        Value::String(text) => text.len(),
        value => pretty_len(value),
    };
    match value {
        Value::Object(fields) if !fields.is_empty() => {
            fields.values().map(|value| block_bytes(text(value))).sum()
        }
        value => block_bytes(text(value)),
    }
}

/// The length of `value` as indented JSON, counted without writing it.
pub(super) fn pretty_len(value: &Value) -> usize {
    let mut counter = Counter(0);
    _ = serde_json::to_writer_pretty(&mut counter, value);
    counter.0
}

/// Counts the bytes written to it.
struct Counter(usize);

impl io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
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
