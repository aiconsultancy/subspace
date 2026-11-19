//! What the watcher decides, apart from the requests it makes: which tasks
//! to poll, when the whole list is due again, and what an alert reads.

use std::collections::HashMap;
use std::time::Duration;

use subspace_framework::core::task_state::{AlertKind, alert_for};

use crate::model::AgentRow;

/// How often the full list reloads while anything is running.
pub const LIST_REFRESH: Duration = Duration::from_secs(5 * 60);
/// The most tasks one poll round asks about, so a long list cannot turn
/// into a burst of requests.
pub const MAX_POLLED: usize = 10;

/// The task ids a poll round asks about: the monitored cloud tasks that are
/// still running, newest first, capped.
pub fn to_poll(rows: &[AgentRow]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.is_watched())
        .take(MAX_POLLED)
        .map(|row| row.id.clone())
        .collect()
}

/// Whether anything on the list can still change on its own.
pub fn any_active(rows: &[AgentRow]) -> bool {
    rows.iter()
        .any(|row| row.group() != crate::model::StateGroup::Finished)
}

/// The alert a task's new state earns, given the state the watcher last saw.
/// A task with no previous state never alerts, so a first load is silent.
pub fn alert(previous: Option<&String>, current: &str) -> Option<AlertKind> {
    alert_for(previous?, current)
}

/// The states the watcher remembers, for the rows it can see. States of rows
/// that left the list are dropped, so a task that returns starts silent
/// again rather than alerting on stale memory.
pub fn remembered(rows: &[AgentRow]) -> HashMap<String, String> {
    rows.iter()
        .map(|row| (row.id.clone(), row.state.clone()))
        .collect()
}

/// What a toast and a notification read for a task that changed.
pub fn alert_text(name: &str, repo: &str, kind: AlertKind) -> String {
    let what = if name.trim().is_empty() {
        "A task"
    } else {
        name
    };
    if repo.is_empty() {
        format!("{what} {}", kind.label())
    } else {
        format!("{what} ({repo}) {}", kind.label())
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::core::task_state::AlertKind;

    use super::{MAX_POLLED, alert, alert_text, any_active, remembered, to_poll};
    use crate::model::{AgentRow, Kind};

    fn row(id: &str, state: &str, monitor: bool) -> AgentRow {
        AgentRow {
            key: format!("cloud:{id}"),
            kind: Kind::Cloud,
            id: id.into(),
            title: format!("Fix {id}"),
            repo: "Owner/one".into(),
            state: state.into(),
            created_at: String::new(),
            updated_at: String::new(),
            monitor,
            subject: None,
            html_url: String::new(),
            pull_request: None,
        }
    }

    #[test]
    fn only_monitored_running_cloud_tasks_are_polled() {
        let mut local = row("s", "busy", true);
        local.kind = Kind::Local;
        let rows = vec![
            row("watched", "in_progress", true),
            row("unwatched", "in_progress", false),
            row("done", "completed", true),
            local,
        ];
        assert_eq!(to_poll(&rows), ["watched"]);
    }

    #[test]
    fn a_long_list_polls_only_the_first_rows() {
        let rows: Vec<AgentRow> = (0..MAX_POLLED + 5)
            .map(|n| row(&n.to_string(), "queued", true))
            .collect();
        assert_eq!(to_poll(&rows).len(), MAX_POLLED);
        assert!(any_active(&rows));
        assert!(!any_active(&[row("done", "completed", true)]));
        assert!(!any_active(&[]));
    }

    #[test]
    fn a_task_the_watcher_has_not_seen_before_never_alerts() {
        assert_eq!(alert(None, "completed"), None);
        assert_eq!(
            alert(Some(&"in_progress".to_owned()), "completed"),
            Some(AlertKind::Completed)
        );
        assert_eq!(alert(Some(&"completed".to_owned()), "completed"), None);
        assert_eq!(alert(Some(&"completed".to_owned()), "failed"), None);
    }

    #[test]
    fn the_watcher_remembers_only_the_rows_it_can_see() {
        let states = remembered(&[row("a", "queued", true), row("b", "completed", false)]);
        assert_eq!(states.len(), 2);
        assert_eq!(states.get("a").map(String::as_str), Some("queued"));
        assert!(remembered(&[]).is_empty());
    }

    #[test]
    fn an_alert_names_the_task_its_repository_and_what_happened() {
        assert_eq!(
            alert_text("Fix the login button", "Owner/one", AlertKind::Completed),
            "Fix the login button (Owner/one) completed"
        );
        assert_eq!(
            alert_text("", "Owner/one", AlertKind::NeedsInput),
            "A task (Owner/one) needs input"
        );
        assert_eq!(alert_text("Fix it", "", AlertKind::Failed), "Fix it failed");
    }
}
