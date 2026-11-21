//! The navigator: the Space's views, who made the decisions and the
//! projects, each a row that changes the list, with the signed-in account
//! in its footer. Which row is selected and what each counts is pure.

use gpui_kit::{
    ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::model::{DecisionListItem, Project};
use subspace_framework::dashboard::{Dashboard as _, workspace};
use subspace_framework::lucide::IconName;

use crate::Decisions;
use crate::listing;
use crate::model::{Maker, ProjectChoice, View};
use crate::words;

/// One navigator row: the choice it makes, its label, its count and
/// whether it is the current one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry<T> {
    pub(crate) value: T,
    pub(crate) label: String,
    pub(crate) count: Option<String>,
    pub(crate) selected: bool,
}

/// The decisions the counts are taken from, and whether there are more.
#[derive(Clone, Copy)]
pub(crate) struct Loaded<'a> {
    pub(crate) items: &'a [DecisionListItem],
    pub(crate) more: bool,
}

/// A row's count. A section on its first row has read every decision the
/// other rows split, so each row shows one; otherwise only the chosen row
/// shows its count, which is the list's.
fn mark(
    loaded: Option<Loaded<'_>>,
    shown: bool,
    count: impl Fn(&[DecisionListItem]) -> usize,
) -> Option<String> {
    let loaded = loaded.filter(|_| shown)?;
    let total = count(loaded.items);
    Some(if loaded.more {
        format!("{total}+")
    } else {
        total.to_string()
    })
}

pub(crate) fn views(current: View, loaded: Option<Loaded<'_>>) -> Vec<Entry<View>> {
    let every = current == View::All;
    View::ALL
        .into_iter()
        .map(|view| Entry {
            value: view,
            label: view.label().to_owned(),
            count: mark(loaded, every || view == current, |items| {
                view.status()
                    .map_or(items.len(), |status| listing::count_status(items, status))
            }),
            selected: view == current,
        })
        .collect()
}

pub(crate) fn makers(current: Maker, loaded: Option<Loaded<'_>>) -> Vec<Entry<Maker>> {
    let anyone = current == Maker::Anyone;
    Maker::ALL
        .into_iter()
        .map(|maker| Entry {
            value: maker,
            label: maker.label().to_owned(),
            count: mark(loaded, anyone || maker == current, |items| {
                maker
                    .kind()
                    .map_or(items.len(), |kind| listing::count_maker(items, kind))
            }),
            selected: maker == current,
        })
        .collect()
}

/// Every project first, then each active project, and the chosen one even
/// when it is archived or no longer listed.
pub(crate) fn projects(
    current: &ProjectChoice,
    projects: &[Project],
    loaded: Option<Loaded<'_>>,
) -> Vec<Entry<ProjectChoice>> {
    let every = *current == ProjectChoice::All;
    let mut entries = vec![whole_list(
        ProjectChoice::All,
        "All projects".to_owned(),
        every,
        loaded,
    )];
    entries.extend(
        projects
            .iter()
            .filter(|project| {
                project.archived_at.is_none() || current.project_key() == Some(project.key.as_str())
            })
            .map(|project| {
                let value = ProjectChoice::Project(project.key.clone());
                let chosen = value == *current;
                Entry {
                    count: mark(loaded, every || chosen, |items| {
                        listing::count_project(items, &project.key)
                    }),
                    label: words::project(&project.key, Some(&project.name)),
                    selected: chosen,
                    value,
                }
            }),
    );
    if let ProjectChoice::Project(key) = current
        && !projects.iter().any(|project| &project.key == key)
    {
        entries.push(whole_list(current.clone(), key.clone(), true, loaded));
    }
    entries
}

/// A project row that counts the whole list read, when it is `selected`.
fn whole_list(
    value: ProjectChoice,
    label: String,
    selected: bool,
    loaded: Option<Loaded<'_>>,
) -> Entry<ProjectChoice> {
    Entry {
        value,
        label,
        count: mark(loaded, selected, <[DecisionListItem]>::len),
        selected,
    }
}

/// The navigator, titled with the Space's name.
pub(crate) fn render(this: &Decisions, cx: &Context<Decisions>) -> workspace::Navigator {
    let loaded = this.listing.items.as_deref().map(|items| Loaded {
        items,
        more: this.listing.more,
    });
    workspace::Navigator::new(Decisions::TITLE)
        .child(workspace::nav_section("Views", cx))
        .children(
            views(this.choice.view, loaded)
                .into_iter()
                .map(|entry| view_row(entry, cx)),
        )
        .child(workspace::nav_section("Made by", cx))
        .children(
            makers(this.choice.maker, loaded)
                .into_iter()
                .map(|entry| maker_row(entry, cx)),
        )
        .child(workspace::nav_section("Projects", cx))
        .children(
            projects(&this.choice.project, &this.session.projects, loaded)
                .into_iter()
                .map(|entry| project_row(entry, cx)),
        )
        .footer(account_line(this))
}

/// The parts every row shares: its id, label, count and selection.
fn row<T>(key: String, entry: Entry<T>) -> workspace::NavRow {
    let row = workspace::NavRow::new(key, entry.label).selected(entry.selected);
    match entry.count {
        Some(count) => row.count(count),
        None => row,
    }
}

fn view_row(entry: Entry<View>, cx: &Context<Decisions>) -> workspace::NavRow {
    let view = entry.value;
    row(format!("decisions-view:{}", view.key()), entry)
        .icon(view.icon())
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.choose_view(view, cx);
        }))
}

fn maker_row(entry: Entry<Maker>, cx: &Context<Decisions>) -> workspace::NavRow {
    let maker = entry.value;
    row(format!("decisions-maker:{}", maker.key()), entry)
        .icon(maker.icon())
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.choose_maker(maker, cx);
        }))
}

fn project_row(entry: Entry<ProjectChoice>, cx: &Context<Decisions>) -> workspace::NavRow {
    let project = entry.value.clone();
    let icon = if project == ProjectChoice::All {
        IconName::FolderOpen
    } else {
        IconName::Folder
    };
    row(format!("decisions-project:{}", project.key()), entry)
        .icon(icon)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.choose_project(project.clone(), cx);
        }))
}

/// The signed-in account, cut with an ellipsis in a narrow navigator.
fn account_line(this: &Decisions) -> impl IntoElement + use<> {
    let line = SharedString::from(this.account_line());
    div()
        .id("decisions-account")
        .test_support()
        .aria_label(line.clone())
        .min_w_0()
        .truncate()
        .child(line)
}

#[cfg(test)]
#[path = "tests/navigator.rs"]
mod tests;
