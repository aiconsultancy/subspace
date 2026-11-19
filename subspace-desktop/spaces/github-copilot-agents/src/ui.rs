//! The words and ids the filters, the menus and the commands read as.

use crate::keys::Picking;
use crate::model::AgentRow;

/// The value that stands for "every one of them" in a filter menu.
pub(crate) const ALL: &str = "";

pub(crate) fn menu_title(picking: Picking) -> &'static str {
    picking_words(picking).0
}

pub(crate) fn button_id(picking: Picking) -> &'static str {
    picking_words(picking).1
}

pub(crate) fn shortcut(picking: Picking) -> &'static str {
    picking_words(picking).2
}

/// A filter menu's title, button id and shortcut, in one place per menu.
fn picking_words(picking: Picking) -> (&'static str, &'static str, &'static str) {
    match picking {
        Picking::Kind => ("Kind", "github-copilot-agents-kind", "1"),
        Picking::Group => ("State", "github-copilot-agents-state", "2"),
        Picking::Repo => ("Repository", "github-copilot-agents-repo", "3"),
    }
}

/// What the monitor command reads for the row it would act on.
pub(crate) fn monitor_command(row: &AgentRow) -> &'static str {
    if row.monitor {
        "Stop monitoring this task"
    } else {
        "Monitor this task"
    }
}

pub(crate) fn monitor_message(title: &str, on: bool) -> String {
    let what = if title.trim().is_empty() {
        "the task"
    } else {
        title
    };
    if on {
        format!("Monitoring {what}: it will alert when it finishes or asks for an answer")
    } else {
        format!("No longer monitoring {what}")
    }
}

pub(crate) fn toggle_label(what: &str, on: bool) -> String {
    format!("{what}: {}", if on { "on" } else { "off" })
}
#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agents::{Subject, SubjectKind};

    use super::{button_id, menu_title, monitor_command, monitor_message, shortcut, toggle_label};
    use crate::keys::Picking;
    use crate::model::{AgentRow, Kind};

    fn row(monitor: bool) -> AgentRow {
        AgentRow {
            key: "cloud:t1".into(),
            kind: Kind::Cloud,
            id: "t1".into(),
            title: "Fix the login button".into(),
            repo: "Owner/one".into(),
            state: "in_progress".into(),
            created_at: String::new(),
            updated_at: String::new(),
            monitor,
            subject: Some(Subject {
                kind: SubjectKind::Chat,
                id: "t1".into(),
            }),
            html_url: String::new(),
            pull_request: None,
        }
    }

    #[test]
    fn the_monitor_command_offers_the_change_not_the_state() {
        assert_eq!(monitor_command(&row(false)), "Monitor this task");
        assert_eq!(monitor_command(&row(true)), "Stop monitoring this task");
    }

    #[test]
    fn the_monitor_message_names_the_task_it_changed() {
        assert!(monitor_message("Fix it", true).starts_with("Monitoring Fix it"));
        assert_eq!(
            monitor_message("  ", false),
            "No longer monitoring the task"
        );
    }

    #[test]
    fn each_filter_has_its_own_title_button_and_key() {
        let all = [Picking::Kind, Picking::Group, Picking::Repo];
        let titles: Vec<&str> = all.iter().copied().map(menu_title).collect();
        let ids: Vec<&str> = all.iter().copied().map(button_id).collect();
        let keys: Vec<&str> = all.iter().copied().map(shortcut).collect();
        assert_eq!(titles, ["Kind", "State", "Repository"]);
        assert_eq!(
            ids,
            [
                "github-copilot-agents-kind",
                "github-copilot-agents-state",
                "github-copilot-agents-repo"
            ]
        );
        assert_eq!(keys, ["1", "2", "3"]);
    }

    #[test]
    fn a_toggle_reads_its_current_setting() {
        assert_eq!(toggle_label("Alert sound", true), "Alert sound: on");
        assert_eq!(toggle_label("Alert sound", false), "Alert sound: off");
    }
}
