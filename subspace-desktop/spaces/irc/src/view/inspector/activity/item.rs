//! One item in full: a model step with its settled tokens and cost, a
//! tool call with its input as parsed and as streamed, progress, output
//! and errors, or reply and reasoning text. Output of a type this version
//! does not know shows as its JSON. Its blocks are those
//! [`super::blocks`] lists, so what is drawn is what was measured.

use gpui_kit::{AnyElement, Context};
use subspace_client_fleet::activity::{
    ActivityEntry, ActivityItem, StepItem, TextItem, ToolItem, UnknownItem,
};

use super::blocks::whole_blocks;
use super::parts::{Draw, body_block, caption, card};
use super::text::visible;
use crate::Irc;

/// An entry held whole: its label, a note on what it is, and its blocks.
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
    card(id, &label(entry), parts, cx)
}

/// A step's tokens and cost, or why an unknown item shows as its JSON.
fn note(id: &str, entry: &ActivityEntry, cx: &Context<Irc>) -> Option<AnyElement> {
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

/// The label of an item's card, as drawn above its body.
pub(super) fn label(entry: &ActivityEntry) -> String {
    match entry {
        ActivityEntry::Item(item) => match &**item {
            ActivityItem::Step(step) => step_label(step),
            ActivityItem::Text(text) => words_label("Reply", text),
            ActivityItem::Reasoning(text) => words_label("Reasoning", text),
            ActivityItem::Tool(tool) => tool_label(tool),
        },
        ActivityEntry::Unknown(unknown) => unknown_label(unknown),
        ActivityEntry::Paged(paged) => super::paged::label(paged),
    }
}

fn unknown_label(unknown: &UnknownItem) -> String {
    visible(&format!("Item · {}", unknown.kind))
}

fn words_label(label: &str, text: &TextItem) -> String {
    match text.status.as_str() {
        "completed" => label.to_owned(),
        status => format!("{label} · {}", visible(status)),
    }
}

fn step_label(step: &StepItem) -> String {
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
    visible(&label)
}

fn tool_label(tool: &ToolItem) -> String {
    let name = tool.name.as_deref().unwrap_or("unnamed tool");
    visible(&format!("Tool · {name} · {}", tool.status))
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
