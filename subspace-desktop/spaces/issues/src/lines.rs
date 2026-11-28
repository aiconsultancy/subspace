//! The list's lines: state groups, issue rows and a group's *more* line,
//! as the shared list draws them, and which groups start folded.

use std::collections::{HashMap, HashSet};

use gpui_kit::SharedString;
use subspace_api_client::model::IssueListItem;
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use crate::listing::{IssueGroup, More};
use crate::model::{StateKind, SubIssues};

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

/// Each issue row's key, with its group and position.
pub(crate) fn row_index(groups: &[IssueGroup]) -> HashMap<SharedString, (usize, usize)> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(group, found)| {
            found
                .rows
                .iter()
                .enumerate()
                .map(move |(row, item)| (issue_key(&item.fields.id), (group, row)))
        })
        .collect()
}

/// Keep long titles readable in a narrow pane, even in a deep hierarchy.
const MAX_INDENT: u8 = 4;

#[derive(Clone, Copy)]
struct Source<'a> {
    group: usize,
    offset: usize,
    item: &'a IssueListItem,
}

/// The projected group rows and the rails that continue below each nested row.
pub(crate) struct Projection {
    pub(crate) groups: Vec<Group>,
    pub(crate) guides: HashMap<SharedString, u8>,
}

pub(crate) fn list_groups(groups: &[IssueGroup], mode: SubIssues, folded: &[bool]) -> Projection {
    let source: Vec<_> = groups
        .iter()
        .enumerate()
        .flat_map(|(group, rows)| rows.rows.iter().map(move |item| (group, item)))
        .enumerate()
        .map(|(offset, (group, item))| Source {
            group,
            offset,
            item,
        })
        .collect();
    let children = match mode {
        SubIssues::Hidden => HashMap::new(),
        SubIssues::Nested => children_by_parent(&source),
    };
    let mut layout = Layout {
        guides: HashMap::new(),
        branches: vec![Vec::new(); source.len()],
        folded,
        moved: vec![0; groups.len()],
    };
    layout.place(&source, &children);
    let mut rows = vec![Vec::new(); groups.len()];
    for (root, branch) in source.iter().zip(layout.branches) {
        rows[root.group].extend(branch);
    }
    Projection {
        guides: layout.guides,
        groups: groups
            .iter()
            .zip(rows)
            .zip(layout.moved)
            .map(|((group, rows), moved)| group_line(group, rows, moved))
            .collect(),
    }
}

/// Link only issues already returned by this view, so a search or filter can
/// show a matching child even when its parent did not match or is on a later page.
fn children_by_parent<'a>(source: &[Source<'a>]) -> HashMap<&'a str, Vec<Source<'a>>> {
    let ids: HashSet<_> = source
        .iter()
        .map(|source| source.item.fields.id.as_str())
        .collect();
    let mut children = HashMap::<&str, Vec<_>>::new();
    for &row in source {
        if let Some(parent) = row
            .item
            .fields
            .parent_id
            .as_deref()
            .filter(|id| ids.contains(id))
        {
            children.entry(parent).or_default().push(row);
        }
    }
    children
}

struct Layout<'a> {
    guides: HashMap<SharedString, u8>,
    branches: Vec<Vec<Row>>,
    folded: &'a [bool],
    moved: Vec<u32>,
}

impl Layout<'_> {
    fn place<'a>(&mut self, source: &[Source<'a>], children: &HashMap<&'a str, Vec<Source<'a>>>) {
        let nested: HashSet<_> = children
            .values()
            .flatten()
            .map(|source| source.item.fields.id.as_str())
            .collect();
        let mut placed = HashSet::new();
        for &root in source
            .iter()
            .filter(|row| !nested.contains(row.item.fields.id.as_str()))
        {
            self.branch(root, children, &mut placed);
        }
        // Independently paged groups may briefly disagree during a parent edit.
        // Give any rootless rows a starting point so every result stays reachable.
        for &root in source {
            if !placed.contains(root.item.fields.id.as_str()) {
                self.branch(root, children, &mut placed);
            }
        }
    }

    fn branch<'a>(
        &mut self,
        root: Source<'a>,
        children: &HashMap<&'a str, Vec<Source<'a>>>,
        placed: &mut HashSet<&'a str>,
    ) {
        let mut next = vec![(root, root, 0_u8, 0_u8)];
        while let Some((row, mut anchor, mut indent, mut guides)) = next.pop() {
            let fields = &row.item.fields;
            let id = fields.id.as_str();
            if !placed.insert(id) {
                continue;
            }
            // A fold hides its own group, even when it contains a parent.
            // Keep matching children from open groups reachable as roots.
            if self.folded[anchor.group] && !self.folded[row.group] {
                (anchor, indent, guides) = (row, 0, 0);
            }
            let key = issue_key(id);
            if indent > 0 {
                self.moved[row.group] += 1;
                self.guides.insert(key.clone(), guides);
            }
            let done = StateKind::parse(&fields.state.kind).is_terminal();
            self.branches[anchor.offset].push(Row {
                indent,
                ..line_row(key, fields.title.clone().into(), done)
            });
            if let Some(children) = children.get(id) {
                let depth = (indent + 1).min(MAX_INDENT);
                let rail = 1 << (depth - 1);
                let ancestors = guides & (rail - 1);
                next.extend(children.iter().enumerate().rev().map(|(index, &child)| {
                    let continues = if index + 1 < children.len() { rail } else { 0 };
                    (child, anchor, depth, ancestors | continues)
                }));
            }
        }
    }
}

/// Keep the heading count and pagination in their source state. The heading
/// counts top-level entries, so subtract the rows already attached elsewhere.
fn group_line(group: &IssueGroup, mut rows: Vec<Row>, moved: u32) -> Group {
    rows.extend(
        more_title(group.more)
            .map(|title| line_row(format!("{MORE}{}", group.key).into(), title.into(), false)),
    );
    Group {
        key: group_key(&group.key),
        icon: blank(),
        name: group.name.clone().into(),
        count: group
            .total
            .map(|total| total.saturating_sub(moved).to_string().into()),
        rows,
    }
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

/// The next page when selection approaches the end of the displayed group.
pub(crate) fn visible_page_near<'a>(
    groups: &'a [Group],
    key: &str,
    distance: usize,
) -> Option<&'a str> {
    groups.iter().find_map(|group| {
        let (last, issues) = group.rows.split_last()?;
        if Some(last.title.as_ref()) != more_title(More::Available) {
            return None;
        }
        let Line::More(state) = line(&last.key)? else {
            return None;
        };
        issues
            .iter()
            .rev()
            .take(distance)
            .any(|row| row.key == key)
            .then_some(state)
    })
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
