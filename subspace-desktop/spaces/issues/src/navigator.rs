//! What the navigator lists: the views, the projects in scope and the
//! initiatives, and which row of each is the current one. Choosing a row
//! makes the board's existing change of view, project or initiative; from
//! the issue page it shows the list first.

use gpui_kit::{Context, Window};
use subspace_framework::dashboard::menu::MenuOption;
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::Issues;
use crate::model::{ProjectScope, Tab};

/// Activity's key, beside the tabs' stored keys.
const ACTIVITY_KEY: &str = "activity";

/// What the pane lists: one of the four issue lists, or the activity feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Tab(Tab),
    Activity,
}

impl View {
    /// The views in the navigator's order, which is also the keys `1`–`5`.
    pub(crate) const ALL: [View; 5] = [
        View::Tab(Tab::MyIssues),
        View::Tab(Tab::Active),
        View::Tab(Tab::Backlog),
        View::Tab(Tab::All),
        View::Activity,
    ];

    /// The view that shows: the activity feed covers the chosen tab.
    pub(crate) fn current(tab: Tab, activity: bool) -> View {
        if activity {
            View::Activity
        } else {
            View::Tab(tab)
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            View::Tab(tab) => tab.key(),
            View::Activity => ACTIVITY_KEY,
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<View> {
        View::ALL.into_iter().find(|view| view.key() == key)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            View::Tab(tab) => tab.spec().label,
            View::Activity => "Activity",
        }
    }

    pub(crate) fn icon(self) -> SvgIcon {
        match self {
            View::Tab(tab) => tab.spec().icon,
            View::Activity => IconName::Activity,
        }
    }
}

/// One view row: My issues carries the count of the person's open issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewEntry {
    pub(crate) view: View,
    pub(crate) count: Option<String>,
    pub(crate) selected: bool,
}

/// The Views section's rows, with `current` selected.
pub(crate) fn views(current: View, my_issues: Option<u32>) -> Vec<ViewEntry> {
    View::ALL
        .into_iter()
        .map(|view| ViewEntry {
            view,
            count: my_issues
                .filter(|_| view == View::Tab(Tab::MyIssues))
                .map(|count| count.to_string()),
            selected: view == current,
        })
        .collect()
}

/// A project or initiative row: the value its choice passes on, its label,
/// and whether it is the current one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) selected: bool,
}

/// The Projects section's rows from the picker's options: every project
/// first when it is offered, then each active project in scope.
pub(crate) fn projects(
    options: impl Iterator<Item = (ProjectScope, String)>,
    current: &ProjectScope,
) -> Vec<Entry> {
    let (all, named): (Vec<_>, Vec<_>) =
        options.partition(|(scope, _)| *scope == ProjectScope::AllProjects);
    all.into_iter()
        .chain(named)
        .map(|(scope, label)| Entry {
            value: scope.key().to_owned(),
            selected: scope == *current,
            label,
        })
        .collect()
}

/// The Initiatives section's rows from the picker's options. A lone option
/// is no choice to make, so it lists nothing.
pub(crate) fn initiatives(options: &[MenuOption]) -> Vec<Entry> {
    if options.len() < 2 {
        return Vec::new();
    }
    options
        .iter()
        .map(|option| Entry {
            value: option.value.to_string(),
            label: option.label.to_string(),
            selected: option.checked,
        })
        .collect()
}

// qual:allow(coupling, oi) reason: "A navigator choice joins leaving the issue page to the board's existing view, project and initiative changes, which keep their own modules."
impl Issues {
    /// A choice made beside the issue page shows the list first. False while
    /// an unsaved field keeps the page open.
    fn leave_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.issue_view.snapshot.is_none() {
            return true;
        }
        if self.inline_navigation_blocked(cx) {
            return false;
        }
        self.close_detail(cx);
        self.focus_list(window, cx);
        true
    }

    pub(crate) fn choose_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if !self.leave_page(window, cx) {
            return;
        }
        match view {
            View::Tab(tab) => self.set_tab(tab, cx),
            View::Activity => self.open_activity(window, cx),
        }
    }

    pub(crate) fn choose_project(
        &mut self,
        project: ProjectScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.leave_page(window, cx) {
            self.set_project(project, cx);
        }
    }

    /// `value` is the initiative picker's option value.
    pub(crate) fn choose_initiative(
        &mut self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.leave_page(window, cx) {
            self.initiatives
                .update(cx, |catalog, cx| catalog.pick(value, cx));
        }
    }
}

#[cfg(test)]
#[path = "tests/navigator.rs"]
mod tests;
