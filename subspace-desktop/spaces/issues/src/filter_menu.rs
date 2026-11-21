//! Assignee, Priority and Filters use the shared anchored, searchable menu.
//! Assignee and Priority are named by their labels alone; Filters wears the
//! funnel glyph.

use gpui_kit::{AnyElement, Context, Window};
use subspace_framework::dashboard::action::{filter_button, menu_button};
use subspace_framework::dashboard::menu::{MenuContents, MenuEvent, MenuOption};
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::Issues;
use crate::model::{IssuePriority, ProjectScope, Tab};

const ALL_ASSIGNEES: &str = "assignee:";
const ALL_ASSIGNEES_LABEL: &str = "All assignees";

#[derive(Clone, Copy)]
pub(crate) enum FilterMenu {
    Assignee,
    Priority,
    More,
}

impl FilterMenu {
    fn facts(self) -> (&'static str, &'static str, Option<SvgIcon>) {
        match self {
            Self::Assignee => ("issues-assignee-button", "Assignee", None),
            Self::Priority => ("issues-priority-button", "Priority", None),
            Self::More => ("issues-filters-button", "Filters", Some(IconName::Funnel)),
        }
    }

    pub(crate) fn options(self, issues: &Issues) -> Vec<MenuOption> {
        match self {
            Self::Assignee => assignees(issues),
            Self::Priority => priorities(issues),
            Self::More => more(issues),
        }
    }
}

pub(crate) fn button(menu: FilterMenu, issues: &Issues, cx: &mut Context<Issues>) -> AnyElement {
    let (id, label, icon) = menu.facts();
    let active = match menu {
        FilterMenu::Assignee => issues.tab == Tab::MyIssues || issues.filters.assignee.is_some(),
        FilterMenu::Priority => !issues.filters.priorities.is_empty(),
        FilterMenu::More => issues.filters.has_more(),
    };
    let button = match icon {
        Some(icon) => menu_button(id, icon, label, active),
        None => filter_button(id, label, active),
    }
    .on_click(cx.listener(move |this, _, window, cx| open(menu, this, window, cx)));
    issues.menu_anchors.trigger(id, button, cx)
}

pub(crate) fn open(
    menu: FilterMenu,
    issues: &mut Issues,
    window: &mut Window,
    cx: &mut Context<Issues>,
) {
    let (id, label, _) = menu.facts();
    let kept = !matches!(menu, FilterMenu::Assignee);
    let contents = MenuContents::new(label, "Find an option", menu.options(issues)).keep_open(kept);
    let anchor = issues.menu_anchors.anchor(id, cx);
    let Some(entity) = issues.menu_anchors.open(anchor, contents, window, cx) else {
        return;
    };
    cx.subscribe(&entity, move |this, entity, event: &MenuEvent, cx| {
        match event {
            MenuEvent::Pick(value) => {
                if this.tab == Tab::MyIssues && value.as_ref() == ALL_ASSIGNEES {
                    this.set_tab(Tab::All, cx);
                } else {
                    this.filters.pick(value);
                    this.rescope(cx);
                }
                if kept {
                    let options = menu.options(this);
                    entity.update(cx, |menu, cx| menu.set_options(options, cx));
                } else {
                    this.menu = None;
                }
            }
            MenuEvent::Close => this.menu = None,
            MenuEvent::Query(_) => {}
        }
        cx.notify();
    })
    .detach();
    issues.menu = Some(entity);
    cx.notify();
}

fn assignees(issues: &Issues) -> Vec<MenuOption> {
    if issues.tab == Tab::MyIssues {
        return vec![
            MenuOption::new("assignee:me", "Me").checked(true),
            MenuOption::new(ALL_ASSIGNEES, ALL_ASSIGNEES_LABEL),
        ];
    }
    [
        ("", ALL_ASSIGNEES_LABEL),
        ("me", "Me"),
        ("none", "No assignee"),
    ]
    .into_iter()
    .map(|(value, label)| {
        MenuOption::new(format!("assignee:{value}"), label)
            .checked(issues.filters.assignee.as_deref().unwrap_or_default() == value)
    })
    .collect()
}

fn priorities(issues: &Issues) -> Vec<MenuOption> {
    let mut options = (0..=i32::from(crate::filters::MAX_PRIORITY))
        .map(|value| {
            MenuOption::new(
                format!("priority:{value}"),
                IssuePriority::from_value(value).look().label,
            )
            .checked(issues.filters.priorities.contains(&(value as u8)))
        })
        .collect::<Vec<_>>();
    options.push(
        MenuOption::new("priorities", "Any priority").checked(issues.filters.priorities.is_empty()),
    );
    options
}

fn more(issues: &Issues) -> Vec<MenuOption> {
    let filters = &issues.filters;
    let mut options = statuses(issues);
    options.push(section(
        "status",
        "Any status",
        "Status",
        filters.states.is_empty() && filters.state_types.is_empty(),
    ));
    for (value, label) in [
        ("", "All types"),
        ("issue", "Issues"),
        ("tracker", "Trackers"),
    ] {
        options.push(section(
            &format!("type:{value}"),
            label,
            "Type",
            filters.kind.as_deref().unwrap_or_default() == value,
        ));
    }
    for (value, label) in [
        ("", "Any due date"),
        ("overdue", "Overdue"),
        ("thisWeek", "Due this week"),
        ("none", "No due date"),
    ] {
        options.push(section(
            &format!("due:{value}"),
            label,
            "Due date",
            filters.due.as_deref().unwrap_or_default() == value,
        ));
    }
    options.push(section(
        "archived",
        "Show archived",
        "Visibility",
        filters.archived,
    ));
    options
}

fn section(value: &str, label: &str, heading: &str, checked: bool) -> MenuOption {
    MenuOption {
        section: Some(heading.to_owned().into()),
        ..MenuOption::new(value.to_owned(), label.to_owned()).checked(checked)
    }
}

fn allowed(tab: Tab, kind: &str) -> bool {
    match tab {
        Tab::Active => matches!(kind, "unstarted" | "started"),
        Tab::Backlog => kind == "backlog",
        Tab::MyIssues | Tab::All => true,
    }
}

fn statuses(issues: &Issues) -> Vec<MenuOption> {
    if let ProjectScope::Project(key) = &issues.project {
        return issues
            .projects
            .iter()
            .filter(|project| &project.key == key)
            .flat_map(|project| &project.states)
            .filter(|state| allowed(issues.tab, &state.kind))
            .map(|state| {
                section(
                    &format!("state:{}", state.id),
                    &state.name,
                    "Status",
                    issues.filters.states.contains(&state.id),
                )
            })
            .collect();
    }
    [
        ("triage", "Triage"),
        ("backlog", "Backlog"),
        ("unstarted", "Todo"),
        ("started", "In progress"),
        ("completed", "Done"),
        ("canceled", "Cancelled"),
        ("duplicate", "Duplicate"),
    ]
    .into_iter()
    .filter(|(kind, _)| allowed(issues.tab, kind))
    .map(|(kind, label)| {
        section(
            &format!("stateType:{kind}"),
            label,
            "Status",
            issues.filters.state_types.iter().any(|value| value == kind),
        )
    })
    .collect()
}
