//! The navigator: the Space's views, then the groups of the list on show,
//! with the signed-in account under them.

use std::collections::HashMap;

use gpui_kit::{
    ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::list::{self, Group};
use subspace_framework::dashboard::{Dashboard as _, workspace};
use subspace_framework::lucide::IconName;

use crate::{Actions, model, model::View};

/// One list group as a navigator row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupRow {
    /// The list group's key, which a click selects.
    pub key: SharedString,
    pub label: SharedString,
    pub count: SharedString,
    /// The group's header or one of its rows is the list's selection.
    pub selected: bool,
    /// Nothing in the group matches, so its row reads quietly.
    pub empty: bool,
}

/// What the list's groups are in `view`, as their section's title. Flow
/// draws a graph in place of the list, so it has no section.
pub(crate) fn groups_title(view: View, by_status: bool) -> Option<&'static str> {
    match view {
        View::Runs if by_status => Some("Statuses"),
        View::Runs => Some("Workflows"),
        View::Deployments | View::Releases => Some("Repositories"),
        View::Flow => None,
    }
}

/// The count beside a view. Runs carries the loaded runs, as the status
/// line counts them; the other views carry none.
pub(crate) fn view_count(view: View, runs: usize) -> Option<String> {
    (view == View::Runs && runs > 0).then(|| runs.to_string())
}

/// A group's name without the separator a missing workflow leaves behind,
/// such as `octo/app` for `octo/app · `.
fn tidy(name: &str) -> &str {
    name.trim_end_matches([' ', '·'])
}

/// A group's name without its repository's owner, such as `app · CI` for
/// `octo/app · CI`.
fn without_owner(name: &str) -> &str {
    match model::group_repo(name).find('/') {
        Some(slash) => &name[slash + 1..],
        None => name,
    }
}

/// Each group's label: its name without the owner, or its whole name when
/// two owners have a group of that name.
fn labels(groups: &[Group]) -> Vec<SharedString> {
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for group in groups {
        *uses.entry(tidy(without_owner(&group.name))).or_default() += 1;
    }
    groups
        .iter()
        .map(|group| {
            let short = tidy(without_owner(&group.name));
            let label = if uses.get(short).copied() == Some(1) {
                short
            } else {
                tidy(&group.name)
            };
            SharedString::from(label.to_owned())
        })
        .collect()
}

/// Whether `group` holds the list's selection: its header or one of its rows.
fn holds(group: &Group, selected: Option<&SharedString>) -> bool {
    selected.is_some_and(|key| &group.key == key || group.rows.iter().any(|row| &row.key == key))
}

/// The navigator rows of the list's groups, in list order.
pub(crate) fn group_rows(groups: &[Group], selected: Option<&SharedString>) -> Vec<GroupRow> {
    groups
        .iter()
        .zip(labels(groups))
        .map(|(group, label)| GroupRow {
            key: group.key.clone(),
            label,
            count: group
                .count
                .clone()
                .unwrap_or_else(|| group.rows.len().to_string().into()),
            selected: holds(group, selected),
            empty: group.rows.is_empty(),
        })
        .collect()
}

pub(crate) fn render(this: &Actions, cx: &Context<Actions>) -> workspace::Navigator {
    let navigator = workspace::Navigator::new(Actions::TITLE)
        .child(workspace::nav_section("Views", cx))
        .children(View::ALL.map(|view| view_row(this, view, cx)));
    let groups = this.list.groups();
    let navigator = match groups_title(this.view, this.filters.by_status) {
        Some(title) if !groups.is_empty() => {
            navigator.child(workspace::nav_section(title, cx)).children(
                group_rows(groups, this.list.selected())
                    .into_iter()
                    .map(|row| group_row(row, cx)),
            )
        }
        _ => navigator,
    };
    navigator.footer(account_line(this))
}

/// A view as a row: the current one is selected, and a click changes view.
fn view_row(this: &Actions, view: View, cx: &Context<Actions>) -> workspace::NavRow {
    let row = workspace::NavRow::new(format!("actions-view-{}", view.title()), view.title())
        .icon(view.icon())
        .selected(this.view == view)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_view(view, cx)));
    match view_count(view, this.runs().len()) {
        Some(count) => row.count(count),
        None => row,
    }
}

fn group_row(row: GroupRow, cx: &Context<Actions>) -> workspace::NavRow {
    let key = row.key.clone();
    workspace::NavRow::new(format!("actions-group-{}", row.key), row.label)
        .icon(IconName::ListTree)
        .count(row.count)
        .selected(row.selected)
        .quiet(row.empty)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            reveal_group(this, key.clone(), cx);
        }))
}

/// Shows a list group from the navigator: unfolds it, selects its header and
/// scrolls the list to it.
fn reveal_group(this: &mut Actions, key: SharedString, cx: &mut Context<Actions>) {
    list::update(this, cx, |list| {
        if list.is_folded(&key) {
            list.toggle(&key);
        }
        list.select(key);
    });
}

/// The signed-in account, cut with an ellipsis in a narrow navigator.
fn account_line(this: &Actions) -> impl IntoElement + use<> {
    let line = SharedString::from(this.account().line());
    div()
        .id("actions-account")
        .test_support()
        .aria_label(line.clone())
        .min_w_0()
        .truncate()
        .child(line)
}

#[cfg(test)]
mod tests {
    use gpui_kit::SharedString;
    use subspace_framework::dashboard::list::{Group, Row};
    use subspace_framework::dashboard::{Glyph, Tone};

    use super::{group_rows, groups_title, view_count};
    use crate::model::View;

    fn group(name: &str, rows: &[&str]) -> Group {
        Group {
            key: name.to_owned().into(),
            icon: Glyph::new("◇", Tone::Muted),
            name: name.to_owned().into(),
            count: Some(rows.len().to_string().into()),
            rows: rows.iter().copied().map(row).collect(),
        }
    }

    fn row(key: &str) -> Row {
        Row {
            key: key.to_owned().into(),
            icon: Glyph::new("●", Tone::Green),
            id: None,
            title: "CI #1".into(),
            trailing: Vec::new(),
            done: false,
            indent: 0,
        }
    }

    fn labels(groups: &[Group]) -> Vec<String> {
        group_rows(groups, None)
            .into_iter()
            .map(|row| row.label.to_string())
            .collect()
    }

    #[test]
    fn the_section_is_named_for_what_the_list_groups_by() {
        assert_eq!(groups_title(View::Runs, false), Some("Workflows"));
        assert_eq!(groups_title(View::Runs, true), Some("Statuses"));
        assert_eq!(groups_title(View::Deployments, true), Some("Repositories"));
        assert_eq!(groups_title(View::Releases, false), Some("Repositories"));
        assert_eq!(groups_title(View::Flow, false), None, "Flow draws no list");
    }

    #[test]
    fn only_runs_carries_a_count_and_only_once_runs_are_loaded() {
        assert_eq!(view_count(View::Runs, 602).as_deref(), Some("602"));
        assert_eq!(view_count(View::Runs, 0), None);
        assert_eq!(view_count(View::Deployments, 602), None);
        assert_eq!(view_count(View::Flow, 602), None);
    }

    #[test]
    fn groups_keep_list_order_with_their_counts_and_drop_the_owner() {
        let groups = [
            group("octo/web · Deploy", &["a", "b"]),
            group("octo/web · CI", &["c"]),
            group("octo/api", &[]),
        ];
        let rows = group_rows(&groups, None);
        let counts: Vec<String> = rows.iter().map(|row| row.count.to_string()).collect();
        let empty: Vec<bool> = rows.iter().map(|row| row.empty).collect();
        assert_eq!(labels(&groups), ["web · Deploy", "web · CI", "api"]);
        assert_eq!(counts, ["2", "1", "0"]);
        assert_eq!(empty, [false, false, true], "an empty group is quiet");
        assert_eq!(rows[1].key, "octo/web · CI", "a click selects this key");
    }

    #[test]
    fn two_owners_with_one_group_name_keep_their_owners() {
        let groups = [
            group("octo/app · CI", &["a"]),
            group("acme/app · CI", &["b"]),
            group("acme/app · Deploy", &["c"]),
            group("octo/app · ", &["d"]),
        ];
        assert_eq!(
            labels(&groups),
            ["octo/app · CI", "acme/app · CI", "app · Deploy", "app"],
            "a missing workflow leaves no separator behind"
        );
    }

    #[test]
    fn the_group_that_holds_the_selection_is_selected() {
        let groups = [
            group("octo/web · Deploy", &["a", "b"]),
            group("octo/web · CI", &["c"]),
            group("octo/api", &[]),
        ];
        let selected = |key: &str| {
            let key = SharedString::from(key.to_owned());
            group_rows(&groups, Some(&key))
                .iter()
                .map(|row| row.selected)
                .collect::<Vec<_>>()
        };
        assert_eq!(selected("b"), [true, false, false], "one of its rows");
        assert_eq!(selected("octo/api"), [false, false, true], "its header");
        assert_eq!(selected("elsewhere"), [false; 3]);
        assert!(group_rows(&groups, None).iter().all(|row| !row.selected));
    }
}
