//! The list's lines: state groups, issue rows and a group's *more* line,
//! as the shared list draws them, and which groups start folded.

use gpui_kit::SharedString;
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use crate::listing::{IssueGroup, More};
use crate::model::StateKind;

const GROUP: &str = "group:";
const ISSUE: &str = "issue:";
const MORE: &str = "more:";

/// What a list key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line<'a> {
    /// A state group, by its key.
    Group(&'a str),
    /// An issue, by its id.
    Issue(&'a str),
    /// A group's *more* line, by the group's key.
    More(&'a str),
}

pub(crate) fn line(key: &str) -> Option<Line<'_>> {
    if let Some(group) = key.strip_prefix(GROUP) {
        return Some(Line::Group(group));
    }
    if let Some(id) = key.strip_prefix(ISSUE) {
        return Some(Line::Issue(id));
    }
    key.strip_prefix(MORE).map(Line::More)
}

pub(crate) fn group_key(group: &str) -> SharedString {
    format!("{GROUP}{group}").into()
}

pub(crate) fn issue_key(id: &str) -> SharedString {
    format!("{ISSUE}{id}").into()
}

/// The groups as the shared list draws them. Their marks, lanes and the
/// identifier come from the board's list hooks, so the glyphs stay empty.
pub(crate) fn list_groups(groups: &[IssueGroup]) -> Vec<Group> {
    groups
        .iter()
        .map(|group| Group {
            key: group_key(&group.key),
            icon: blank(),
            name: group.name.clone().into(),
            count: group.total.map(|total| total.to_string().into()),
            rows: group_rows(group),
        })
        .collect()
}

fn group_rows(group: &IssueGroup) -> Vec<Row> {
    let done = group.kind.is_terminal();
    let issues = group.rows.iter().map(|item| {
        line_row(
            issue_key(&item.fields.id),
            item.fields.title.clone().into(),
            done,
        )
    });
    let more = more_title(group.more)
        .map(|title| line_row(format!("{MORE}{}", group.key).into(), title.into(), false));
    issues.chain(more).collect()
}

/// A row whose marks and lanes the board's list hooks draw.
fn line_row(key: SharedString, title: SharedString, done: bool) -> Row {
    Row {
        key,
        icon: blank(),
        id: None,
        title,
        trailing: Vec::new(),
        done,
        indent: 0,
    }
}

/// The *more* line's words, when the group shows one.
pub(crate) fn more_title(more: More) -> Option<&'static str> {
    match more {
        More::Available => Some("Show more"),
        More::Loading => Some("Loading more…"),
        More::Failed => Some("Couldn't load more"),
        More::None | More::Unread => None,
    }
}

fn blank() -> Glyph {
    Glyph::new("", Tone::Muted)
}

/// Saved folds: the groups folded by hand, and the Done, Cancelled and
/// Duplicate groups unfolded by hand. Keys are state ids or types only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Folds {
    pub(crate) folded: Vec<String>,
    pub(crate) unfolded: Vec<String>,
}

impl Folds {
    /// Done, Cancelled and Duplicate start folded; a hand fold wins.
    pub(crate) fn is_folded(&self, key: &str, kind: StateKind) -> bool {
        let has = |list: &[String]| list.iter().any(|saved| saved == key);
        has(&self.folded) || (kind.is_terminal() && !has(&self.unfolded))
    }

    /// Records a fold or unfold, keeping only what differs from the default.
    pub(crate) fn record(&mut self, key: &str, kind: StateKind, folded: bool) {
        self.folded.retain(|saved| saved != key);
        self.unfolded.retain(|saved| saved != key);
        match (folded, kind.is_terminal()) {
            (true, false) => self.folded.push(key.to_owned()),
            (false, true) => self.unfolded.push(key.to_owned()),
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "tests/lines.rs"]
mod tests;
