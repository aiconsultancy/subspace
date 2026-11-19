//! Summaries of loaded replies, built once when a conversation synchronises.

use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, Local, Utc};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    AnyElement, InteractiveElement as _, IntoElement as _, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_client_irc::{BufferKey, Target, ThreadRoots};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::panel::IconName;

use super::Row;
use crate::lines::Item;
use crate::state::{Buffer, Entry, is_message};

pub(crate) type Counts = BTreeMap<i64, Summary>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Summary {
    count: usize,
    latest: DateTime<Utc>,
}

/// Counts never include other buffers, missing roots, or malformed chains.
/// The one parent map serves every line; rendering rows only reads the result.
pub(crate) fn counts(buffer: &BufferKey, held: &Buffer) -> Counts {
    let roots = ThreadRoots::new(buffer, held.entries.iter().map(|entry| &entry.line));
    let summaries = summaries(held, &roots);
    root_counts(buffer, held, &summaries)
}

/// Remeasure changed roots without replacing surviving rows or their scroll anchor.
pub(crate) fn changed_rows(
    rows: &[Row],
    entries: &VecDeque<Entry>,
    before: &Counts,
    after: &Counts,
) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| match row {
            Row::Item(Item::Message { at, .. }) => {
                let seq = entries[*at].seq;
                (before.get(&seq) != after.get(&seq)).then_some(index)
            }
            _ => None,
        })
        .collect()
}

fn summaries<'a>(held: &'a Buffer, roots: &ThreadRoots<'a>) -> BTreeMap<&'a str, Summary> {
    let mut summaries: BTreeMap<&str, Summary> = BTreeMap::new();
    for entry in &held.entries {
        let line = &entry.line;
        if line.reply_to.is_none() || !is_message(line.kind) {
            continue;
        }
        if let Some(root) = roots.root(line) {
            let summary = summaries.entry(root).or_insert(Summary {
                count: 0,
                latest: line.time,
            });
            summary.count += 1;
            summary.latest = summary.latest.max(line.time);
        }
    }
    summaries
}

fn root_counts(buffer: &BufferKey, held: &Buffer, summaries: &BTreeMap<&str, Summary>) -> Counts {
    held.entries
        .iter()
        .filter(|_| buffer.target != Target::Server)
        .filter_map(|entry| {
            let line = &entry.line;
            if line.buffer != *buffer || line.reply_to.is_some() || !is_message(line.kind) {
                return None;
            }
            let summary = summaries.get(line.id.as_deref()?)?;
            Some((entry.seq, summary.clone()))
        })
        .collect()
}

/// Read-only metadata under the root, aligned with the message body.
pub(super) fn metadata(entry: &Entry, counts: &Counts, tokens: &Tokens) -> Option<AnyElement> {
    let summary = counts.get(&entry.seq)?;
    let noun = if summary.count == 1 {
        "reply"
    } else {
        "replies"
    };
    let count = format!("{} {noun}", summary.count);
    let latest = format!(
        "Last reply {}",
        summary.latest.with_timezone(&Local).format("%H:%M"),
    );
    let label = format!("{count} · {latest}");
    Some(
        h_flex()
            .id(SharedString::from(format!(
                "irc-replies:{}",
                entry.line.id.as_deref()?
            )))
            .role(Role::Label)
            .aria_label(label.clone())
            .test_support()
            .flex_wrap()
            .gap_2()
            .py_1()
            .text_size(tokens.caption_size())
            .text_color(tokens.muted)
            .child(
                h_flex()
                    .gap_1()
                    .text_color(tokens.fg)
                    .child(Icon::from(IconName::MessagesSquare).xsmall())
                    .child(count),
            )
            .child(div().child(latest))
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests;
