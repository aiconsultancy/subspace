//! How many bytes of the captain's activity the view holds, counted
//! against its budget ([`super::load`], [`super::draw`]): the snapshot's
//! inline content, every text, input, output and value in UTF-8 bytes;
//! and what an item's body mounts while drawn, each block in the bytes it
//! draws, markers and all, or one part when it is long, plus the
//! formatted text kept for it.

use std::io;

use serde_json::Value;
use subspace_client_fleet::activity::{
    ActivityContent, ActivityEntry, ActivityError, ActivityItem, ActivityTokens, AgentActivity,
    ToolItem,
};

use super::blocks::{Block, Source, error_text};
use super::parts::PART;
use super::text::visible_len;

/// What a number, a boolean or null counts for.
const SCALAR: u64 = 8;

/// About how many bytes the snapshot's inline content holds: the length
/// of every text, input, output and value in it, and the token counts
/// this version does not know that each turn's use keeps, shown or not
/// (SUB decision `084f5236` item 3).
pub(super) fn inline_bytes(activity: &AgentActivity) -> u64 {
    activity
        .turns
        .iter()
        .map(|turn| {
            let usage = turn.summary.usage.as_ref();
            let items: u64 = turn.items.iter().map(entry_bytes).sum();
            items + usage.map_or(0, |usage| tokens_bytes(&usage.tokens))
        })
        .sum()
}

/// The token counts this version does not know, kept so they can still
/// be shown: every retained byte is charged (SUB decision `084f5236` item
/// 3). Those it knows are numbers inside the item.
fn tokens_bytes(tokens: &ActivityTokens) -> u64 {
    map_bytes(&tokens.extra)
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

/// Every string a known item holds, its fields this version does not
/// know too (SUB decision `33ccecf6` item 4).
fn item_bytes(item: &ActivityItem) -> u64 {
    match item {
        ActivityItem::Text(words) | ActivityItem::Reasoning(words) => {
            text(&words.status) + text(&words.text) + map_bytes(&words.extra)
        }
        ActivityItem::Step(step) => {
            let model = step.model.as_ref().map_or(0, |model| {
                text(&model.provider)
                    + text(&model.id)
                    + model.variant.as_deref().map_or(0, text)
                    + map_bytes(&model.extra)
            });
            text(&step.status)
                + model
                + step.finish.as_deref().map_or(0, text)
                + step.tokens.as_ref().map_or(0, tokens_bytes)
                + error_bytes(step.error.as_ref())
                + map_bytes(&step.extra)
        }
        ActivityItem::Tool(tool) => tool_bytes(tool),
    }
}

fn tool_bytes(tool: &ToolItem) -> u64 {
    let content: u64 = tool.content.iter().flatten().map(content_bytes).sum();
    text(&tool.call_id)
        + tool.name.as_deref().map_or(0, text)
        + text(&tool.status)
        + text(&tool.raw_input)
        + tool.input.as_ref().map_or(0, value_bytes)
        + tool.metadata.as_ref().map_or(0, value_bytes)
        + content
        + error_bytes(tool.error.as_ref())
        + map_bytes(&tool.extra)
}

fn content_bytes(content: &ActivityContent) -> u64 {
    match content {
        ActivityContent::Text { text: words, extra } => text(words) + map_bytes(extra),
        ActivityContent::File {
            uri,
            mime,
            name,
            extra,
        } => text(uri) + text(mime) + name.as_deref().map_or(0, text) + map_bytes(extra),
        ActivityContent::Other(raw) => value_bytes(raw),
    }
}

fn error_bytes(error: Option<&ActivityError>) -> u64 {
    error.map_or(0, |error| {
        text(&error.kind)
            + text(&error.message)
            + error.response_body.as_deref().map_or(0, text)
            + map_bytes(&error.extra)
    })
}

fn text(text: &str) -> u64 {
    text.len() as u64
}

fn map_bytes(fields: &serde_json::Map<String, Value>) -> u64 {
    fields
        .iter()
        .map(|(key, value)| text(key) + value_bytes(value))
        .sum()
}

/// About how many bytes a JSON value holds: its strings and keys, and
/// eight for any other value.
fn value_bytes(value: &Value) -> u64 {
    match value {
        Value::String(text) => text.len() as u64,
        Value::Array(values) => values.iter().map(value_bytes).sum(),
        Value::Object(fields) => map_bytes(fields),
        _ => SCALAR,
    }
}

/// What the body of an item drawn whole holds, in UTF-8 bytes: each
/// block as it draws, markers and all ([`visible_len`]), or one part when
/// it is long, plus the formatted text kept for it while it is drawn (an
/// indented JSON value, an error). Counted once per version of the item
/// ([`super::draw`]), never while drawing.
pub(super) fn body_bytes(blocks: &[Block<'_>]) -> u64 {
    count_measure();
    blocks.iter().map(block_bytes).sum()
}

fn block_bytes(block: &Block<'_>) -> u64 {
    let (kept, shown) = source_lens(block.source);
    kept.saturating_add(drawn(shown, block.long)) as u64
}

/// What a block drawing `shown` bytes mounts: all of them, or one part
/// of at most [`PART`] when they pass `long`.
pub(super) fn drawn(shown: usize, long: usize) -> usize {
    if shown > long { PART } else { shown }
}

/// The bytes kept formatted for `source` while it is drawn, and the bytes
/// it draws.
fn source_lens(source: Source<'_>) -> (usize, usize) {
    match source {
        Source::Text(text) => (0, visible_len(text)),
        Source::Json(value) => json_lens(value),
        Source::Error(error) => {
            let text = error_text(error);
            (text.len(), visible_len(&text))
        }
    }
}

/// The length of `value` as indented JSON, and the bytes that draws as,
/// counted without writing it.
fn json_lens(value: &Value) -> (usize, usize) {
    let mut shown = 0;
    let raw = {
        let mut sink = Utf8Sink::new(|text: &str| shown += visible_len(text));
        _ = serde_json::to_writer_pretty(&mut sink, value);
        sink.raw
    };
    (raw, shown)
}

/// The most bytes one character takes in UTF-8.
const MAX_UTF8: usize = 4;

/// Takes what is written to it as text, a character never split, and
/// counts its bytes.
pub(super) struct Utf8Sink<F> {
    pub(super) raw: usize,
    /// The start of a character whose other bytes are still to come.
    pending: Vec<u8>,
    take: F,
}

impl<F: FnMut(&str)> Utf8Sink<F> {
    pub(super) fn new(take: F) -> Self {
        Self {
            raw: 0,
            pending: Vec::new(),
            take,
        }
    }
}

impl<F: FnMut(&str)> io::Write for Utf8Sink<F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.raw += bytes.len();
        let mut rest = bytes;
        // Finish a character begun in the last write, a byte at a time.
        while !self.pending.is_empty() {
            let Some((&byte, after)) = rest.split_first() else {
                return Ok(bytes.len());
            };
            self.pending.push(byte);
            rest = after;
            // Whole, or not UTF-8 at all (never from JSON): taken.
            let incomplete =
                std::str::from_utf8(&self.pending).is_err_and(|error| error.error_len().is_none());
            if !incomplete {
                (self.take)(&String::from_utf8_lossy(&self.pending));
                self.pending.clear();
            }
        }
        let valid = match std::str::from_utf8(rest) {
            Ok(text) => text.len(),
            Err(error) => error.valid_up_to(),
        };
        let (text, tail) = rest.split_at(valid);
        (self.take)(&String::from_utf8_lossy(text));
        if tail.len() < MAX_UTF8 {
            self.pending.extend_from_slice(tail);
        } else {
            // Never from JSON, which is UTF-8: drawn replaced.
            (self.take)(&String::from_utf8_lossy(tail));
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
thread_local! {
    static MEASURED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Counts a body measured or a block prepared, for a test to see that
/// drawing again does neither.
pub(super) fn count_measure() {
    #[cfg(test)]
    MEASURED.with(|measured| measured.set(measured.get() + 1));
}

/// How many bodies were measured and blocks prepared on this thread.
#[cfg(test)]
pub(crate) fn measured() -> u64 {
    MEASURED.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use subspace_client_fleet::activity::{ActivityEntry, AgentActivity};

    use super::super::blocks::whole_blocks;
    use super::{body_bytes, inline_bytes};

    /// Round 5 thermo (P1), SUB decision `33ccecf6` item 2: a tool's name
    /// and an argument's key are its body, measured as they draw, each
    /// marker twelve bytes; the fixed labels above them are not.
    #[test]
    fn names_and_keys_are_measured_in_the_body() {
        let key = "\u{202E}".repeat(24);
        let mut input = serde_json::Map::new();
        input.insert(key, serde_json::json!("v"));
        let entry: ActivityEntry = serde_json::from_value(serde_json::json!({
            "id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "tool",
            "callId": "call-1", "name": "N".repeat(100), "status": "completed",
            "rawInput": "", "input": input, "executed": true, "metadata": null,
            "content": [], "error": null
        }))
        .unwrap();
        assert_eq!(body_bytes(&whole_blocks(&entry)), 100 + 24 * 12 + 1);
    }

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

    /// Round 7 delta (P2), SUB decision `084f5236` item 3: the token counts
    /// this version does not know that a turn's use keeps are charged too,
    /// though only its total is shown.
    #[test]
    fn a_turns_use_charges_the_token_counts_this_version_does_not_know() {
        let activity = |tokens| -> AgentActivity {
            serde_json::from_value(serde_json::json!({
                "agent": {"id": "aurora-lyra", "role": "captain", "backend": "opencode",
                          "collected": true},
                "turns": [{
                    "id": "t-1", "seq": 1, "status": "completed", "startedAt": 1,
                    "updatedAt": 2, "endedAt": 2, "reconnects": 0,
                    "usage": {"steps": 1, "tokens": tokens, "cost": 0},
                    "items": []
                }],
                "cursor": {"generation": "g1", "seq": 1}
            }))
            .unwrap()
        };
        let known = inline_bytes(&activity(serde_json::json!({"input": 1})));
        let extension = "x".repeat(8192);
        let kept = activity(serde_json::json!({"input": 1, "newCount": extension}));
        assert_eq!(inline_bytes(&kept) - known, 8192 + "newCount".len() as u64);
    }
}
