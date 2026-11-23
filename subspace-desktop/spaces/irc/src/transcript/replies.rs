//! Summaries of loaded replies, built once when a conversation synchronises.

use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use chrono::{DateTime, Local, Utc};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_client_irc::{BufferKey, Target, ThreadRoots};
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::panel::IconName;

use super::{Row, RowContext};
use crate::conversation::ConversationEvent;
use crate::lines::Item;
use crate::state::{Buffer, Entry, is_message};

pub(crate) type Counts = BTreeMap<i64, Summary>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Summary {
    count: usize,
    latest: DateTime<Utc>,
}

impl Summary {
    /// How many replies the root has.
    pub(crate) fn count(&self) -> usize {
        self.count
    }
}

/// Counts never include other buffers, missing roots, or malformed chains.
/// The one parent map serves every line; rendering rows only reads the result.
pub(crate) fn counts(buffer: &BufferKey, held: &Buffer) -> Counts {
    let roots = ThreadRoots::new(buffer, held.entries.iter().map(|entry| &entry.line));
    let summaries = summaries(held, &roots);
    root_counts(buffer, held, &summaries)
}

/// Remeasure changed roots without replacing surviving rows or their scroll
/// anchor. `before` and `after` hold what each root shows under its text,
/// by its sequence: reply counts or request statuses.
pub(crate) fn changed_rows<V: PartialEq>(
    rows: &[Row],
    entries: &VecDeque<Entry>,
    before: &BTreeMap<i64, V>,
    after: &BTreeMap<i64, V>,
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
        let counted = is_message(line.kind) && !held.folded.contains(&entry.seq);
        if line.reply_to.is_none() || !counted {
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

/// What opens a message's thread from the transcript: its reply count, and
/// the handler that asks the host to show the thread.
#[derive(Clone)]
pub(crate) struct ThreadLink {
    pub count: usize,
    pub open: Rc<dyn Fn(&mut Window, &mut App)>,
}

/// The link to `entry`'s thread, when the conversation's inspector has a
/// Thread tab and the message has an ID to name it by.
pub(super) fn link(entry: &Entry, context: &RowContext) -> Option<ThreadLink> {
    if !context.threads {
        return None;
    }
    let id = entry.line.id.clone().filter(|id| !id.is_empty())?;
    let conversation = context.conversation.clone();
    Some(ThreadLink {
        count: context.replies.get(&entry.seq).map_or(0, Summary::count),
        open: Rc::new(move |_, cx| {
            _ = conversation.update(cx, |_, cx| {
                cx.emit(ConversationEvent::OpenThread(id.clone()));
            });
        }),
    })
}

/// `3 replies`, or `1 reply`.
pub(crate) fn replies_text(count: usize) -> String {
    let noun = if count == 1 { "reply" } else { "replies" };
    format!("{count} {noun}")
}

/// Metadata under the root, aligned with the message body: its reply count
/// and when the last came. With a `link` it opens the thread, unless the
/// request's status line (`on_status`) carries the count instead.
pub(super) fn metadata(
    entry: &Entry,
    counts: &Counts,
    (link, on_status): (Option<&ThreadLink>, bool),
    tokens: &Tokens,
) -> Option<AnyElement> {
    if on_status && link.is_some() {
        return None;
    }
    let summary = counts.get(&entry.seq)?;
    let count = replies_text(summary.count);
    let latest = format!(
        "Last reply {}",
        summary.latest.with_timezone(&Local).format("%H:%M"),
    );
    let label = format!("{count} · {latest}");
    let shown = if link.is_some() {
        format!("{count} ›")
    } else {
        count
    };
    let row = h_flex()
        .id(SharedString::from(format!(
            "irc-replies:{}",
            entry.line.id.as_deref()?
        )))
        .test_support();
    let row = match link {
        Some(link) => {
            let open = Rc::clone(&link.open);
            row.role(Role::Button)
                .aria_label(format!("{label}. Opens the thread"))
                .cursor_pointer()
                .on_click(move |_, window, cx| open(window, cx))
        }
        None => row.role(Role::Label).aria_label(label),
    };
    Some(
        row.flex_wrap()
            .gap_2()
            .py_1()
            .text_size(tokens.caption_size())
            .text_color(tokens.muted)
            .child(
                h_flex()
                    .gap_1()
                    .text_color(tokens.fg)
                    .child(Icon::from(IconName::MessagesSquare).xsmall())
                    .child(shown),
            )
            .child(div().child(latest))
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests;
