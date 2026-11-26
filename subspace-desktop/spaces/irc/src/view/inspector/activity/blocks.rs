//! The blocks an item's body is drawn from, in order, each under its own
//! label or none: one list that drawing ([`super::item`]), measuring
//! ([`super::size`]) and drawing in parts ([`super::pieces`]) all read,
//! so what is charged is what is drawn.
//!
//! A label is fixed, structural text this version writes (SUB decision
//! `33ccecf6` item 2): `Input`, `Output`, `Name` and the like. Everything
//! an officer's tools sent, a tool's name, an argument's key, a file's
//! name and type, a model, a status that is not a short word, is a block's
//! text, measured and drawn in parts like any other. Fields this version
//! does not know follow, each as its key and its JSON (item 4), so nothing
//! sent is out of reach.

use serde_json::{Map, Value};
use subspace_client_fleet::activity::{
    ActivityContent, ActivityEntry, ActivityError, ActivityItem, PagedItem, StepItem, StepModel,
    TextItem, ToolItem,
};

use super::Held;
use super::parts::{LONG, PART};

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
    /// What its element's ID adds to the item's: `{item}:{suffix}`. Made of
    /// this version's words and indices only, never of what was sent.
    pub(super) suffix: String,
    /// The small label above it, fixed text; none for a reply's text or a
    /// field's value under its key.
    pub(super) label: Option<&'static str>,
    pub(super) source: Source<'a>,
    /// Longer than this, in bytes drawn, it shows in parts of [`PART`].
    pub(super) long: usize,
}

impl<'a> Block<'a> {
    fn new(suffix: String, label: Option<&'static str>, source: Source<'a>) -> Self {
        Self {
            suffix,
            label,
            source,
            long: LONG,
        }
    }

    fn text(suffix: impl Into<String>, label: &'static str, text: &'a str) -> Self {
        Self::new(suffix.into(), Some(label), Source::Text(text))
    }
}

/// The blocks of an entry's body; for an item read on demand, of what is
/// held of it (`held`): the item once read whole, otherwise what its
/// placeholder names. A page read in parts draws beside them, within its
/// read's reservation ([`super::load`]).
pub(super) fn entry_blocks<'a>(entry: &'a ActivityEntry, held: Option<&'a Held>) -> Vec<Block<'a>> {
    match (entry, held) {
        (ActivityEntry::Paged(_), Some(Held::Whole(read))) => whole_blocks(read),
        (ActivityEntry::Paged(paged), _) => paged_blocks(paged),
        (entry, _) => whole_blocks(entry),
    }
}

/// The blocks of an entry held whole.
pub(super) fn whole_blocks(entry: &ActivityEntry) -> Vec<Block<'_>> {
    match entry {
        ActivityEntry::Item(item) => item_blocks(item),
        ActivityEntry::Unknown(unknown) => json_blocks(&unknown.raw),
        // A page read never decodes as a paged item: drawn as invalid.
        ActivityEntry::Paged(_) => Vec::new(),
    }
}

/// What an item too large to send inline names besides its size: its
/// status when that is not a short word, its kind when this version does
/// not know it, and its name.
pub(super) fn paged_blocks(paged: &PagedItem) -> Vec<Block<'_>> {
    status_block(&paged.status)
        .into_iter()
        .chain(kind_block(&paged.kind))
        .chain(name_block(paged.name.as_deref()))
        .collect()
}

/// A kind this version does not know.
fn kind_block(kind: &str) -> Option<Block<'_>> {
    (!matches!(kind, "tool" | "text" | "reasoning" | "step"))
        .then(|| Block::text("kind", "Kind", kind))
}

/// An item of a kind this version does not know: its JSON.
fn json_blocks(raw: &Value) -> Vec<Block<'_>> {
    vec![Block::new("json".to_owned(), None, Source::Json(raw))]
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
        ActivityItem::Text(words) | ActivityItem::Reasoning(words) => text_blocks(words),
        ActivityItem::Step(step) => step_blocks(step),
        ActivityItem::Tool(tool) => tool_blocks(tool),
    }
}

/// A status as the card's heading shows it: a short word of letters,
/// digits, `-` and `_`, as Stargate sends; anything else is drawn in the
/// body instead ([`status_block`]), so the heading stays bounded.
pub(super) fn short_status(status: &str) -> Option<&str> {
    let short = (1..=SHORT_STATUS).contains(&status.len())
        && status
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    short.then_some(status)
}

/// The most bytes a status the heading shows has.
const SHORT_STATUS: usize = 24;

/// A status the heading cannot show ([`short_status`]).
fn status_block(status: &str) -> Option<Block<'_>> {
    short_status(status)
        .is_none()
        .then(|| Block::text("status", "Status", status))
}

/// A name, when there is one.
fn name_block(name: Option<&str>) -> Option<Block<'_>> {
    name.map(|name| Block::text("name", "Name", name))
}

/// A reply's or reasoning's text, with no label.
fn text_blocks(words: &TextItem) -> Vec<Block<'_>> {
    let mut blocks: Vec<Block<'_>> = status_block(&words.status).into_iter().collect();
    blocks.push(Block::new(
        "text".to_owned(),
        None,
        Source::Text(&words.text),
    ));
    blocks.extend(extra_blocks("", &words.extra));
    blocks
}

/// A model step: its model and how it finished, then its failure.
fn step_blocks(step: &StepItem) -> Vec<Block<'_>> {
    let mut blocks: Vec<Block<'_>> = status_block(&step.status).into_iter().collect();
    blocks.extend(step.model.iter().flat_map(model_blocks));
    blocks.extend(
        step.finish
            .as_deref()
            .map(|finish| Block::text("finish", "Finish", finish)),
    );
    blocks.extend(error_blocks(step.error.as_ref()));
    blocks.extend(extra_blocks("", &step.extra));
    blocks
}

/// The model a step ran on: its provider, ID and variant.
fn model_blocks(model: &StepModel) -> Vec<Block<'_>> {
    let mut blocks = vec![
        Block::text("provider", "Provider", &model.provider),
        Block::text("model", "Model", &model.id),
    ];
    blocks.extend(
        model
            .variant
            .as_deref()
            .map(|variant| Block::text("variant", "Variant", variant)),
    );
    blocks.extend(extra_blocks("model:", &model.extra));
    blocks
}

/// A provider failure, if there was one, then its fields this version
/// does not know.
fn error_blocks(error: Option<&ActivityError>) -> Vec<Block<'_>> {
    let Some(error) = error else {
        return Vec::new();
    };
    let mut blocks = vec![Block::new(
        "error".to_owned(),
        Some("Error"),
        Source::Error(error),
    )];
    blocks.extend(extra_blocks("error:", &error.extra));
    blocks
}

/// A tool's name, its input as parsed and as streamed, then its progress
/// or metadata, its output and its error.
fn tool_blocks(tool: &ToolItem) -> Vec<Block<'_>> {
    let mut blocks: Vec<Block<'_>> = status_block(&tool.status).into_iter().collect();
    blocks.extend(name_block(tool.name.as_deref()));
    blocks.extend(
        tool.input
            .iter()
            .flat_map(|input| value_blocks("input", "Input", input)),
    );
    blocks.extend(raw_block(&tool.raw_input));
    let progress = progress_label(&tool.status);
    blocks.extend(
        tool.metadata
            .iter()
            .flat_map(|metadata| value_blocks("meta", progress, metadata)),
    );
    blocks.extend(
        tool.content
            .iter()
            .flatten()
            .enumerate()
            .flat_map(output_blocks),
    );
    blocks.extend(error_blocks(tool.error.as_ref()));
    blocks.extend(extra_blocks("", &tool.extra));
    blocks
}

/// A tool's input as streamed, when any was.
fn raw_block(raw: &str) -> Option<Block<'_>> {
    (!raw.is_empty()).then(|| Block::text("raw", "Input as streamed", raw))
}

fn progress_label(status: &str) -> &'static str {
    if matches!(status, "input" | "running") {
        "Progress"
    } else {
        "Metadata"
    }
}

/// One part of a tool's output: text, a file with its name and type, or
/// the JSON of a type this version does not know.
fn output_blocks((index, content): (usize, &ActivityContent)) -> Vec<Block<'_>> {
    let suffix = format!("out:{index}");
    match content {
        ActivityContent::Text { text, extra } => output_text(suffix, text, extra),
        ActivityContent::File {
            uri,
            mime,
            name,
            extra,
        } => output_file(suffix, (uri, mime, name.as_deref()), extra),
        ActivityContent::Other(raw) => vec![Block::new(
            suffix,
            Some("Output of a type this version can't show"),
            Source::Json(raw),
        )],
    }
}

/// Text a tool gave, then its fields this version does not know.
fn output_text<'a>(suffix: String, text: &'a str, extra: &'a Map<String, Value>) -> Vec<Block<'a>> {
    let mut blocks = vec![Block::text(suffix.clone(), "Output", text)];
    blocks.extend(extra_blocks(&format!("{suffix}:"), extra));
    blocks
}

/// A file a tool gave: its URI, name and type, then its fields this
/// version does not know.
fn output_file<'a>(
    suffix: String,
    (uri, mime, name): (&'a str, &'a str, Option<&'a str>),
    extra: &'a Map<String, Value>,
) -> Vec<Block<'a>> {
    let mut blocks = vec![Block::text(suffix.clone(), "File", uri)];
    blocks.extend(name.map(|name| Block::text(format!("{suffix}:name"), "Name", name)));
    blocks.push(Block::text(format!("{suffix}:type"), "Type", mime));
    blocks.extend(extra_blocks(&format!("{suffix}:"), extra));
    blocks
}

/// A JSON value: an object field by field, each its key under `label`
/// and then its value; anything else whole under `label`.
fn value_blocks<'a>(suffix: &str, label: &'static str, value: &'a Value) -> Vec<Block<'a>> {
    match value {
        Value::Object(fields) if !fields.is_empty() => field_blocks(suffix, label, fields),
        value => vec![Block::new(
            suffix.to_owned(),
            Some(label),
            json_source(value),
        )],
    }
}

/// Each field of `fields`: its key, as text under `label`, then its value
/// under no label. Their IDs number the fields, never name them.
fn field_blocks<'a>(
    suffix: &str,
    label: &'static str,
    fields: &'a Map<String, Value>,
) -> Vec<Block<'a>> {
    fields
        .iter()
        .enumerate()
        .flat_map(|(index, (key, value))| {
            [
                Block::text(format!("{suffix}:{index}:key"), label, key),
                Block::new(format!("{suffix}:{index}"), None, json_source(value)),
            ]
        })
        .collect()
}

/// The fields a known shape carries that this version does not know.
fn extra_blocks<'a>(prefix: &str, extra: &'a Map<String, Value>) -> Vec<Block<'a>> {
    field_blocks(&format!("{prefix}extra"), "Other field", extra)
}

/// A string as its text, anything else as indented JSON.
fn json_source(value: &Value) -> Source<'_> {
    match value {
        Value::String(text) => Source::Text(text),
        value => Source::Json(value),
    }
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
    error_segments(error).concat()
}

/// A provider failure as the pieces of text it draws, in order: what is
/// formatted ([`error_text`]) and what is drawn in parts
/// ([`super::pieces`]) both read these, so a part never formats the
/// whole failure.
pub(super) fn error_segments(error: &ActivityError) -> Vec<std::borrow::Cow<'_, str>> {
    use std::borrow::Cow;
    let mut segments: Vec<Cow<'_, str>> = vec![
        Cow::Borrowed(error.kind.as_str()),
        Cow::Borrowed(": "),
        Cow::Borrowed(error.message.as_str()),
    ];
    if let Some(status) = error.status {
        segments.push(Cow::Owned(format!("\nStatus {status}")));
    }
    if let Some(wait) = error.retry_after_ms {
        segments.push(Cow::Owned(format!("\nRetry after {wait} ms")));
    }
    if let Some(body) = &error.response_body {
        segments.push(Cow::Borrowed("\n"));
        segments.push(Cow::Borrowed(body.as_str()));
    }
    segments
}
