//! One item in full: a model step with its settled tokens and cost, a
//! tool call with its input as parsed and as streamed, progress, output
//! and errors, or reply and reasoning text. Output of a type this version
//! does not know shows as its JSON. Its blocks are those
//! [`super::blocks`] lists, so what is drawn is what was measured.

use gpui_kit::{AnyElement, Context};
use subspace_client_fleet::activity::{ActivityEntry, ActivityItem, StepItem, TextItem};

use super::blocks::{short_status, whole_blocks};
use super::parts::{Draw, body_block, caption, card};
use crate::Irc;

/// An entry held whole: its heading, a note on what it is, and its blocks.
pub(super) fn whole(
    id: &str,
    entry: &ActivityEntry,
    draw: &Draw<'_>,
    cx: &Context<Irc>,
) -> AnyElement {
    let mut parts: Vec<AnyElement> = note(id, entry, cx).into_iter().collect();
    parts.extend(
        whole_blocks(entry)
            .iter()
            .map(|block| body_block(id, block, draw, cx)),
    );
    card(id, &heading(entry), parts, cx)
}

/// A step's tokens and cost, or why an unknown item shows as its JSON:
/// drawn above its body, whole or in parts alike.
pub(super) fn note(id: &str, entry: &ActivityEntry, cx: &Context<Irc>) -> Option<AnyElement> {
    match entry {
        ActivityEntry::Item(item) => match &**item {
            ActivityItem::Step(step) => Some(caption(&format!("{id}:use"), &step_use(step), cx)),
            _ => None,
        },
        ActivityEntry::Unknown(_) => Some(caption(
            &format!("{id}:unknown"),
            "This version can't show this item, so here is what was sent.",
            cx,
        )),
        ActivityEntry::Paged(_) => None,
    }
}

/// The heading of an item's card: fixed words and its status when that is
/// a short word, never what was sent beyond that (SUB decision `33ccecf6`
/// item 2). Its name, model and the rest are in its body.
pub(super) fn heading(entry: &ActivityEntry) -> String {
    match entry {
        ActivityEntry::Item(item) => match &**item {
            ActivityItem::Step(step) => with_status("Step", &step.status),
            ActivityItem::Text(text) => words_heading("Reply", text),
            ActivityItem::Reasoning(text) => words_heading("Reasoning", text),
            ActivityItem::Tool(tool) => with_status("Tool", &tool.status),
        },
        ActivityEntry::Unknown(_) => "Item".to_owned(),
        ActivityEntry::Paged(paged) => super::paged::heading(paged),
    }
}

/// `kind · status`, or `kind` alone when the status is not a short word.
pub(super) fn with_status(kind: &str, status: &str) -> String {
    match short_status(status) {
        Some(status) => format!("{kind} · {status}"),
        None => kind.to_owned(),
    }
}

/// A reply or reasoning: its status only while it is not complete.
fn words_heading(kind: &str, text: &TextItem) -> String {
    match text.status.as_str() {
        "completed" => kind.to_owned(),
        status => with_status(kind, status),
    }
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
