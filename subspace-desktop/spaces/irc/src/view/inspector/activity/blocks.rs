//! The blocks an item's body is drawn from, in order, each under its own
//! label or none: one list that drawing ([`super::item`]), measuring
//! ([`super::size`]) and drawing in parts ([`super::pieces`]) all read,
//! so what is charged is what is drawn.

use serde_json::Value;
use subspace_client_fleet::activity::{
    ActivityContent, ActivityEntry, ActivityError, ActivityItem, ToolItem,
};

use super::Held;
use super::parts::{LONG, PART};
use super::text::visible;

/// What a block draws: text as it is, or a value formatted for it.
#[derive(Clone, Copy)]
pub(super) enum Source<'a> {
    Text(&'a str),
    /// Drawn as indented JSON.
    Json(&'a Value),
    /// Drawn as its type, message, status and response.
    Error(&'a ActivityError),
}

/// One block of an item's body.
pub(super) struct Block<'a> {
    /// What its element's ID adds to the item's: `{item}:{suffix}`.
    pub(super) suffix: String,
    /// The small label above it, as drawn; none for a reply's text.
    pub(super) label: Option<String>,
    pub(super) source: Source<'a>,
    /// Longer than this, in bytes drawn, it shows in parts of [`PART`].
    pub(super) long: usize,
}

impl<'a> Block<'a> {
    fn new(suffix: String, label: Option<String>, source: Source<'a>) -> Self {
        Self {
            suffix,
            label,
            source,
            long: LONG,
        }
    }
}

/// The blocks of an entry's body; for an item read on demand, of what is
/// held of it (`held`): nothing until it is read.
pub(super) fn entry_blocks<'a>(entry: &'a ActivityEntry, held: Option<&'a Held>) -> Vec<Block<'a>> {
    match (entry, held) {
        (ActivityEntry::Paged(_), Some(Held::Whole(read))) => whole_blocks(read),
        (ActivityEntry::Paged(_), Some(Held::Parts(parts))) => page_blocks(&parts.page.data),
        (ActivityEntry::Paged(_), _) => no_blocks(),
        (entry, _) => whole_blocks(entry),
    }
}

/// The blocks of an entry held whole.
pub(super) fn whole_blocks(entry: &ActivityEntry) -> Vec<Block<'_>> {
    match entry {
        ActivityEntry::Item(item) => item_blocks(item),
        ActivityEntry::Unknown(unknown) => json_blocks(&unknown.raw),
        ActivityEntry::Paged(_) => no_blocks(),
    }
}

/// Nothing to draw: an item not read yet.
fn no_blocks<'a>() -> Vec<Block<'a>> {
    Vec::new()
}

/// An item of a kind this version does not know: its JSON.
fn json_blocks(raw: &Value) -> Vec<Block<'_>> {
    vec![Block::new("json".to_owned(), None, Source::Json(raw))]
}

/// One page of the JSON of an item read part by part.
fn page_blocks(data: &str) -> Vec<Block<'_>> {
    vec![page_block(data)]
}

/// One page of the JSON of an item read part by part. A page holds at
/// most 64 KiB, but its markers can draw more: past [`PART`] it shows in
/// parts of its own, so it never draws more than the page it is.
pub(super) fn page_block(data: &str) -> Block<'_> {
    Block {
        long: PART,
        ..Block::new("json-part".to_owned(), None, Source::Text(data))
    }
}

/// The blocks of an item of a kind this version knows.
pub(super) fn item_blocks(item: &ActivityItem) -> Vec<Block<'_>> {
    match item {
        ActivityItem::Text(words) | ActivityItem::Reasoning(words) => text_blocks(&words.text),
        ActivityItem::Step(step) => error_blocks(step.error.as_ref()),
        ActivityItem::Tool(tool) => tool_blocks(tool),
    }
}

/// A reply's or reasoning's text, with no label.
fn text_blocks(text: &str) -> Vec<Block<'_>> {
    vec![Block::new("text".to_owned(), None, Source::Text(text))]
}

/// A provider failure, if there was one.
fn error_blocks(error: Option<&ActivityError>) -> Vec<Block<'_>> {
    error.into_iter().map(error_block).collect()
}

/// A tool's input as parsed and as streamed, then its progress or
/// metadata, its output and its error.
fn tool_blocks(tool: &ToolItem) -> Vec<Block<'_>> {
    let mut blocks = input_blocks(tool);
    blocks.extend(raw_block(tool));
    blocks.extend(meta_blocks(tool));
    blocks.extend(tool.content.iter().flatten().enumerate().map(output_block));
    blocks.extend(error_blocks(tool.error.as_ref()));
    blocks
}

/// A tool's input as parsed, field by field.
fn input_blocks(tool: &ToolItem) -> Vec<Block<'_>> {
    match &tool.input {
        Some(input) => value_blocks("input", "Input", input),
        None => no_blocks(),
    }
}

/// A tool's input as streamed, when any was.
fn raw_block(tool: &ToolItem) -> Option<Block<'_>> {
    (!tool.raw_input.is_empty()).then(|| {
        Block::new(
            "raw".to_owned(),
            Some("Input as streamed".to_owned()),
            Source::Text(&tool.raw_input),
        )
    })
}

/// A tool's progress while it runs, its metadata once it is done.
fn meta_blocks(tool: &ToolItem) -> Vec<Block<'_>> {
    match &tool.metadata {
        Some(metadata) => value_blocks("meta", progress_label(&tool.status), metadata),
        None => no_blocks(),
    }
}

fn progress_label(status: &str) -> &'static str {
    if matches!(status, "input" | "running") {
        "Progress"
    } else {
        "Metadata"
    }
}

/// One part of a tool's output: text, a file, or the JSON of a type this
/// version does not know.
fn output_block((index, content): (usize, &ActivityContent)) -> Block<'_> {
    let suffix = format!("out:{index}");
    match content {
        ActivityContent::Text { text } => output_text(suffix, text),
        ActivityContent::File { uri, mime, name } => {
            output_file(suffix, uri, file_label(name.as_deref(), mime))
        }
        ActivityContent::Other(raw) => output_other(suffix, raw),
    }
}

fn output_text(suffix: String, text: &str) -> Block<'_> {
    Block::new(suffix, Some("Output".to_owned()), Source::Text(text))
}

/// A file a tool gave: its URI, under its name and type.
fn output_file(suffix: String, uri: &str, label: String) -> Block<'_> {
    Block::new(suffix, Some(visible(&label)), Source::Text(uri))
}

/// `File · notes.txt · text/plain`, as sent.
fn file_label(name: Option<&str>, mime: &str) -> String {
    match name {
        Some(name) => format!("File · {name} · {mime}"),
        None => format!("File · {mime}"),
    }
}

fn output_other(suffix: String, raw: &Value) -> Block<'_> {
    Block::new(
        suffix,
        Some("Output of a type this version can't show".to_owned()),
        Source::Json(raw),
    )
}

/// A JSON value: an object field by field, a string field as its text,
/// anything else as indented JSON.
fn value_blocks<'a>(suffix: &str, label: &str, value: &'a Value) -> Vec<Block<'a>> {
    match value {
        Value::Object(fields) if !fields.is_empty() => fields
            .iter()
            .map(|(key, value)| {
                Block::new(
                    format!("{suffix}:{key}"),
                    Some(visible(&format!("{label} · {key}"))),
                    json_source(value),
                )
            })
            .collect(),
        value => vec![Block::new(
            suffix.to_owned(),
            Some(label.to_owned()),
            json_source(value),
        )],
    }
}

/// A string as its text, anything else as indented JSON.
fn json_source(value: &Value) -> Source<'_> {
    match value {
        Value::String(text) => Source::Text(text),
        value => Source::Json(value),
    }
}

/// A provider failure.
fn error_block(error: &ActivityError) -> Block<'_> {
    Block::new(
        "error".to_owned(),
        Some("Error".to_owned()),
        Source::Error(error),
    )
}

/// The text a block formats, kept while it is drawn; `None` for text drawn
/// as it is.
pub(super) fn formatted(source: Source<'_>) -> Option<String> {
    match source {
        Source::Text(_) => None,
        Source::Json(value) => Some(pretty(value)),
        Source::Error(error) => Some(error_text(error)),
    }
}

/// `value` as indented JSON.
pub(super) fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// A provider failure as drawn: its type, message, status and response.
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
