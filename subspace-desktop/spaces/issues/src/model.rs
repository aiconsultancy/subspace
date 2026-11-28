//! Board-owned choices the views draw: the tab, the project, the sub-issues
//! mode, and the workflow-state and priority vocabularies. Subspace API's records
//! stay slice 1's DTOs; nothing here holds issue content.

use subspace_api_client::ListTab;
use subspace_api_client::model::Project;
use subspace_framework::dashboard::Tone;
use subspace_framework::lucide::{IconName, SvgIcon};

/// The list's tabs in Subspace API's order, with their keys `1`–`4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Tab {
    MyIssues,
    #[default]
    Active,
    Backlog,
    All,
}

/// A tab's label, its stored key, its glyph, Subspace API's wording when it lists
/// nothing, and the hub's tab predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabSpec {
    pub(crate) label: &'static str,
    /// The value stored in the `issues.tab` preference.
    pub(crate) key: &'static str,
    pub(crate) icon: SvgIcon,
    pub(crate) empty: &'static str,
    pub(crate) list: ListTab,
}

/// Each tab, in [`Tab`]'s order.
const TABS: [TabSpec; 4] = [
    TabSpec {
        label: "My issues",
        key: "my-issues",
        icon: IconName::UserRound,
        empty: "No issues assigned to you",
        list: ListTab::MyIssues,
    },
    TabSpec {
        label: "Active",
        key: "active",
        icon: IconName::Contrast,
        empty: "No active issues",
        list: ListTab::Active,
    },
    TabSpec {
        label: "Backlog",
        key: "backlog",
        icon: IconName::CircleDashed,
        empty: "No issues in the backlog",
        list: ListTab::Backlog,
    },
    TabSpec {
        label: "All issues",
        key: "all",
        icon: IconName::List,
        empty: "No issues",
        list: ListTab::All,
    },
];

impl Tab {
    pub(crate) const ALL: [Tab; 4] = [Tab::MyIssues, Tab::Active, Tab::Backlog, Tab::All];

    pub(crate) fn spec(self) -> TabSpec {
        TABS[self as usize]
    }

    pub(crate) fn key(self) -> &'static str {
        self.spec().key
    }

    pub(crate) fn from_key(key: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|tab| tab.key() == key)
    }
}

/// Show sub-issues beneath their parents, or hide them in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SubIssues {
    #[default]
    Hidden,
    Nested,
}

impl SubIssues {
    pub(crate) fn toggled(self) -> SubIssues {
        match self {
            SubIssues::Hidden => SubIssues::Nested,
            SubIssues::Nested => SubIssues::Hidden,
        }
    }

    pub(crate) fn action(self) -> &'static str {
        match self {
            SubIssues::Hidden => "Show sub-issues",
            SubIssues::Nested => "Hide sub-issues",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            SubIssues::Hidden => "hidden",
            SubIssues::Nested => "nested",
        }
    }

    pub(crate) fn from_key(key: &str) -> SubIssues {
        match key {
            "flat" | "nested" => SubIssues::Nested,
            _ => SubIssues::Hidden,
        }
    }
}

/// The project the list reads: one project's key, or every project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectScope {
    Project(String),
    AllProjects,
}

impl ProjectScope {
    /// The value stored in the `issues.project` preference.
    pub(crate) fn key(&self) -> &str {
        match self {
            ProjectScope::Project(key) => key,
            ProjectScope::AllProjects => ALL_PROJECTS_KEY,
        }
    }

    pub(crate) fn from_key(key: &str) -> ProjectScope {
        match key {
            ALL_PROJECTS_KEY => ProjectScope::AllProjects,
            key => ProjectScope::Project(key.to_owned()),
        }
    }
}

const ALL_PROJECTS_KEY: &str = "*";

/// Prefer the only active project; otherwise show all projects.
pub(crate) fn default_project(projects: &[Project]) -> ProjectScope {
    let mut active = projects
        .iter()
        .filter(|project| project.archived_at.is_none());
    match (active.next(), active.next()) {
        (Some(only), None) => ProjectScope::Project(only.key.clone()),
        _ => ProjectScope::AllProjects,
    }
}

/// Preserve a saved active project, including while projects are loading.
pub(crate) fn chosen_project(saved: Option<ProjectScope>, projects: &[Project]) -> ProjectScope {
    match saved {
        Some(ProjectScope::AllProjects) => ProjectScope::AllProjects,
        Some(ProjectScope::Project(key))
            if projects.is_empty()
                || projects
                    .iter()
                    .any(|project| project.key == key && project.archived_at.is_none()) =>
        {
            ProjectScope::Project(key)
        }
        _ => default_project(projects),
    }
}

/// Subspace API's workflow state types, as the list groups and glyphs read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateKind {
    Triage,
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
    Duplicate,
    Other,
}

impl StateKind {
    pub(crate) fn parse(kind: &str) -> StateKind {
        match kind {
            "triage" => StateKind::Triage,
            "backlog" => StateKind::Backlog,
            "unstarted" => StateKind::Unstarted,
            "started" => StateKind::Started,
            "completed" => StateKind::Completed,
            "canceled" | "cancelled" => StateKind::Canceled,
            "duplicate" => StateKind::Duplicate,
            _ => StateKind::Other,
        }
    }

    /// Done, Cancelled and Duplicate groups start folded, and their rows
    /// are read when they are unfolded.
    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            StateKind::Completed | StateKind::Canceled | StateKind::Duplicate
        )
    }

    /// A group's name when the list is grouped by state type, for All
    /// projects, and no workflow names it.
    pub(crate) fn label(self) -> &'static str {
        STATE_LABELS[self as usize]
    }
}

/// [`StateKind`]'s names, in its order, as Subspace API's default workflow names them.
const STATE_LABELS: [&str; 8] = [
    "Triage",
    "Backlog",
    "Todo",
    "In Progress",
    "Done",
    "Cancelled",
    "Duplicate",
    "Other",
];

/// Subspace API's issue priorities: 0 none, then urgent to low.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IssuePriority {
    None,
    Urgent,
    High,
    Medium,
    Low,
}

/// How a priority reads: its name, glyph and tone. The glyph is never the
/// only way to read it: the row's accessible name says the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PriorityLook {
    pub(crate) label: &'static str,
    pub(crate) icon: SvgIcon,
    pub(crate) tone: Tone,
}

impl IssuePriority {
    pub(crate) fn from_value(value: i32) -> IssuePriority {
        match value {
            1 => IssuePriority::Urgent,
            2 => IssuePriority::High,
            3 => IssuePriority::Medium,
            4 => IssuePriority::Low,
            _ => IssuePriority::None,
        }
    }

    pub(crate) fn look(self) -> PriorityLook {
        let (label, icon, tone) = match self {
            IssuePriority::None => ("No priority", IconName::Ellipsis, Tone::Muted),
            IssuePriority::Urgent => ("Urgent", IconName::CircleAlert, Tone::Orange),
            IssuePriority::High => ("High", IconName::SignalHigh, Tone::Muted),
            IssuePriority::Medium => ("Medium", IconName::SignalMedium, Tone::Muted),
            IssuePriority::Low => ("Low", IconName::SignalLow, Tone::Muted),
        };
        PriorityLook { label, icon, tone }
    }
}

#[cfg(test)]
#[path = "tests/model.rs"]
mod tests;
