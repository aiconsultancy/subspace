//! The Copilot Agents dashboard's own actions, key help and stored filter keys.

use gpui_kit::actions;
use subspace_framework::dashboard::KeyHelp;

actions!(
    github_copilot_agents,
    [
        /// Open the selected task's session page on GitHub.
        OpenSession,
        /// Open the selected task's pull request on GitHub.
        OpenPullRequest,
        /// Copy the selected row's link.
        CopyLink,
        /// Turn alerts for the selected task on or off.
        ToggleMonitor,
        /// Name the chat the task came from.
        OpenSubject,
        /// Show the selected local chat session in a chat tab.
        OpenInChat,
        /// Show the selected cloud task's last session log, or hide it.
        ShowLog,
        /// Retry loading the selected cloud task's sessions.
        RetrySessions,
        /// The kind filter menu.
        KindMenu,
        /// The state filter menu.
        GroupMenu,
        /// The repository filter menu.
        RepoMenu,
        /// Clear every filter.
        ClearFilters,
    ]
);

pub(crate) const KEYS: &[KeyHelp] = &[
    KeyHelp::new("j / k", "Next / previous row, shown in the inspector"),
    KeyHelp::new(
        "Enter",
        "Open the task or chat as a tab; on a group, fold or unfold it",
    ),
    KeyHelp::new("1 / 2 / 3", "Filter by kind, state or repository"),
    KeyHelp::new("0", "Clear the filters"),
    KeyHelp::new(
        "o",
        "Open the session in browser, or GitHub Agents if no page is available",
    ),
    KeyHelp::new("p", "Open the pull request on GitHub"),
    KeyHelp::new("y", "Copy the link"),
    KeyHelp::new("m", "Monitor the task, or stop monitoring it"),
    KeyHelp::new("u", "Name the chat the task came from"),
    KeyHelp::new("c", "Open the chat session in a chat tab"),
    KeyHelp::new("l", "Show the task's session log, or hide it"),
    KeyHelp::new("Esc", "Back to the list from an open task"),
];

pub(crate) const HELP_FOOT: &str = concat!(
    "Read-only: the Copilot Agents dashboard never starts, steers or stops a task. ",
    "A monitored task that finishes or asks for an answer raises a toast, a sound and, ",
    "while the window is in the background, a notification. ",
    "Cloud tasks are the work account's tasks of the last 30 days."
);

pub(crate) const SEARCH_PLACEHOLDER: &str = "Search agent tasks and chat sessions";

pub(crate) const KIND_KEY: &str = "github-copilot-agents.kind";
pub(crate) const GROUP_KEY: &str = "github-copilot-agents.state";
pub(crate) const REPO_KEY: &str = "github-copilot-agents.repo";
/// Whether the navigator shows; it does unless this is stored as `0`.
pub(crate) const NAVIGATOR_KEY: &str = "github-copilot-agents.navigator";
/// Whether the inspector shows; it does unless this is stored as `0`.
pub(crate) const INSPECTOR_KEY: &str = "github-copilot-agents.inspector";

/// Which filter a toolbar menu is picking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Picking {
    Kind,
    Group,
    Repo,
}
