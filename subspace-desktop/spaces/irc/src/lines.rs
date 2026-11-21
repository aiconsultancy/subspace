//! What the transcript shows for a buffer's lines: day separators,
//! messages grouped by sender, single server lines, and runs of joins,
//! parts, quits and nick changes condensed into one line. Pure, so the
//! grouping and condensing rules are testable without GPUI.

use std::collections::{HashSet, VecDeque};

use chrono::{Datelike as _, Duration, NaiveDate, TimeZone};
use subspace_client_irc::LineKind;

use crate::state::Entry;

/// A message continues its sender's group when it follows their previous
/// message within this many minutes.
pub(crate) const GROUP_MINUTES: i64 = 5;

/// One transcript row. Indexes point into the buffer's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item {
    Day(NaiveDate),
    /// A message from someone, with their avatar and name unless it
    /// `continues` the group of their previous message.
    Message {
        at: usize,
        continues: bool,
    },
    /// A server, topic, mode or error line: compact, without an avatar.
    Line(usize),
    /// A run of membership lines, `first..=last`.
    Membership {
        first: usize,
        last: usize,
    },
}

/// What kind of row a line makes, before grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Message,
    Line,
    Membership,
}

/// A conversation's message from a person gets a message row; the server
/// buffer keeps every line compact.
fn kind(entry: &Entry, conversation: bool) -> Kind {
    let line = &entry.line;
    match line.kind {
        LineKind::Join | LineKind::Part | LineKind::Quit | LineKind::Nick => Kind::Membership,
        LineKind::Privmsg | LineKind::Action | LineKind::Notice
            if conversation && line.from.is_some() =>
        {
            Kind::Message
        }
        _ => Kind::Line,
    }
}

/// `next` continues the group `previous` belongs to: the same sender, the
/// same kind of message (notices stay apart), within [`GROUP_MINUTES`].
pub(crate) fn continues(previous: &Entry, next: &Entry) -> bool {
    let (before, after) = (&previous.line, &next.line);
    let same_sender = match (&before.from, &after.from) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => false,
    };
    let notice = |kind: LineKind| kind == LineKind::Notice;
    let gap = after.time - before.time;
    same_sender
        && notice(before.kind) == notice(after.kind)
        && gap >= Duration::zero()
        && gap <= Duration::minutes(GROUP_MINUTES)
}

/// The rows for `entries`, dated in `zone`. `conversation` is false for a
/// server buffer, whose lines stay compact. The `folded` lines, request
/// progress that a status line shows instead, make no row.
pub(crate) fn items<Tz: TimeZone>(
    entries: &VecDeque<Entry>,
    folded: &HashSet<i64>,
    zone: &Tz,
    conversation: bool,
) -> Vec<Item> {
    let shown: Vec<(usize, &Entry)> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !folded.contains(&entry.seq))
        .collect();
    let kinds: Vec<Kind> = shown
        .iter()
        .map(|(_, entry)| kind(entry, conversation))
        .collect();
    let dates: Vec<NaiveDate> = shown
        .iter()
        .map(|(_, entry)| entry.line.time.with_timezone(zone).date_naive())
        .collect();
    let links: Vec<bool> = shown
        .windows(2)
        .map(|pair| continues(pair[0].1, pair[1].1))
        .collect();
    let positions: Vec<usize> = shown.iter().map(|(at, _)| *at).collect();
    assemble(&positions, &kinds, &dates, &links)
}

/// The rows from each shown line's entry position, kind and date, and
/// whether each shown line continues the one before it
/// (`links[index - 1]`). A membership run spans only adjacent entries.
fn assemble(positions: &[usize], kinds: &[Kind], dates: &[NaiveDate], links: &[bool]) -> Vec<Item> {
    let mut items = Vec::new();
    for (index, ((kind, date), &at)) in kinds.iter().zip(dates).zip(positions).enumerate() {
        let new_day = index == 0 || dates.get(index - 1) != Some(date);
        if new_day {
            items.push(Item::Day(*date));
        }
        let linked = index > 0 && links.get(index - 1).copied().unwrap_or_default();
        match (items.last_mut(), kind) {
            (Some(Item::Membership { last, .. }), Kind::Membership) if *last + 1 == at => {
                *last = at;
            }
            (_, Kind::Membership) => items.push(Item::Membership {
                first: at,
                last: at,
            }),
            (Some(Item::Message { .. }), Kind::Message) if linked => items.push(Item::Message {
                at,
                continues: true,
            }),
            (_, Kind::Message) => items.push(Item::Message {
                at,
                continues: false,
            }),
            (_, Kind::Line) => items.push(Item::Line(at)),
        }
    }
    items
}

/// A stable identity for `item`: it changes only when the row's content does.
pub(crate) fn item_id(item: Item, entries: &VecDeque<Entry>) -> String {
    let seq = |index: usize| entries.get(index).map_or(0, |entry| entry.seq);
    match item {
        Item::Day(date) => format!("day:{date}"),
        Item::Message { at, continues } if continues => format!("message:{}:more", seq(at)),
        Item::Message { at, .. } => format!("message:{}", seq(at)),
        Item::Line(index) => format!("line:{}", seq(index)),
        Item::Membership { first, last } => format!("members:{}:{}", seq(first), seq(last)),
    }
}

/// A day separator's label: `Today`, `Yesterday`, else the weekday and
/// date, with the year when it is not `today`'s.
pub(crate) fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    let yesterday = today.pred_opt();
    if date == today {
        "Today".to_owned()
    } else if Some(date) == yesterday {
        "Yesterday".to_owned()
    } else if date.year() == today.year() {
        date.format("%A %-d %B").to_string()
    } else {
        date.format("%A %-d %B %Y").to_string()
    }
}

/// `alice, bob joined · carol left · dave quit · erin is now erin2`.
pub(crate) fn membership_summary(entries: &[&Entry]) -> String {
    let mut groups: Vec<(LineKind, Vec<String>)> = Vec::new();
    for entry in entries {
        let name = actor(entry);
        match groups.iter_mut().find(|(kind, _)| *kind == entry.line.kind) {
            Some((_, names)) if !names.contains(&name) => names.push(name),
            Some(_) => {}
            None => groups.push((entry.line.kind, vec![name])),
        }
    }
    groups
        .into_iter()
        .map(|(kind, names)| phrase(kind, &names))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Who a membership line is about. A nick change reads `old → new`, the
/// new nick being the text's last word (`is now known as new`).
fn actor(entry: &Entry) -> String {
    let from = entry.line.from.clone().unwrap_or_default();
    let text = subspace_client_irc::format::strip(&entry.line.text);
    let new = text.split_whitespace().next_back().unwrap_or_default();
    match entry.line.kind {
        LineKind::Nick if !new.is_empty() => format!("{from} → {new}"),
        _ if from.is_empty() => text,
        _ => from,
    }
}

fn phrase(kind: LineKind, names: &[String]) -> String {
    let names = names.join(", ");
    match kind {
        LineKind::Join => format!("{names} joined"),
        LineKind::Part => format!("{names} left"),
        LineKind::Quit => format!("{names} quit"),
        _ => names,
    }
}

#[cfg(test)]
mod tests;
