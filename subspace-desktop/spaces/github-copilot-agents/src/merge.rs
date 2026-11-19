//! Building the list: turning cloud tasks, saved task references and local
//! chat sessions into rows, and putting them in the order the list shows.

use std::collections::HashMap;

use subspace_framework::client_github::copilot_agents::Task;
use subspace_framework::core::copilot_agent_tasks::SubjectTaskRef;
use subspace_framework::dashboard::chat_sessions::ChatSession;

use crate::dates;
use crate::model::{
    AgentRow, Kind, LOCAL_BUSY, LOCAL_OPEN, LOCAL_SAVED, StateGroup, UNKNOWN, cloud_key, local_key,
    parse_subject,
};

/// The row one cloud task makes, with the saved reference's monitor flag and
/// subject when the file holds one. The task is taken by value so its fields
/// move into the row.
fn cloud_row(task: Task, saved: Option<&SubjectTaskRef>) -> AgentRow {
    let updated_at = dates::latest(&task.updated_at, &task.created_at);
    let subject = task
        .subject
        .or_else(|| saved.and_then(|saved| parse_subject(&saved.subject)));
    AgentRow {
        key: cloud_key(&task.id),
        kind: Kind::Cloud,
        id: task.id,
        title: task.name,
        repo: task.repo,
        state: task.state,
        created_at: task.created_at,
        updated_at,
        // A task found only through the task list is not monitored: the user
        // turns it on per task.
        monitor: saved.is_some_and(|saved| saved.task.monitor),
        subject,
        html_url: task.html_url,
        pull_request: task.pull_request,
    }
}

/// The row a saved reference makes on its own, before anything loaded it.
/// It shows what the file holds and nothing invented.
fn saved_row(saved: &SubjectTaskRef) -> AgentRow {
    let entry = saved.task.clone();
    AgentRow {
        key: cloud_key(&entry.task_id),
        kind: Kind::Cloud,
        title: format!("Task {}", entry.task_id),
        id: entry.task_id,
        repo: entry.repo,
        state: UNKNOWN.to_owned(),
        updated_at: entry.created_at.clone(),
        created_at: entry.created_at,
        monitor: entry.monitor,
        subject: parse_subject(&saved.subject),
        html_url: String::new(),
        pull_request: None,
    }
}

/// Every cloud row: the tasks GitHub listed, then the saved references that
/// the list left out, so a task Subspace started always has a row. `known`
/// holds the tasks a detail load or a poll filled in since.
pub fn cloud_rows(
    tasks: &[Task],
    saved: &[SubjectTaskRef],
    known: &HashMap<String, Task>,
) -> Vec<AgentRow> {
    let by_id: HashMap<&str, &SubjectTaskRef> = saved
        .iter()
        .map(|entry| (entry.task.task_id.as_str(), entry))
        .collect();
    let mut rows: Vec<AgentRow> = tasks
        .iter()
        .map(|task| {
            let latest = known.get(&task.id).unwrap_or(task).clone();
            cloud_row(latest, by_id.get(task.id.as_str()).copied())
        })
        .collect();
    let listed: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    for entry in saved {
        if listed.contains(&entry.task.task_id) {
            continue;
        }
        rows.push(match known.get(&entry.task.task_id) {
            Some(task) => cloud_row(task.clone(), Some(entry)),
            None => saved_row(entry),
        });
    }
    rows
}

/// The state word a local session reports: whether its chat tab is running.
fn local_state(session: &ChatSession) -> &'static str {
    match (session.busy, session.open) {
        (true, _) => LOCAL_BUSY,
        (false, true) => LOCAL_OPEN,
        (false, false) => LOCAL_SAVED,
    }
}

/// Every local row, from the chat panel's published sessions.
pub fn local_rows(sessions: &[ChatSession]) -> Vec<AgentRow> {
    sessions
        .iter()
        .map(|session| AgentRow {
            key: local_key(&session.id),
            kind: Kind::Local,
            id: session.id.clone(),
            title: session.title.clone(),
            repo: String::new(),
            state: local_state(session).to_owned(),
            created_at: session.created_at.clone(),
            updated_at: dates::latest(&session.modified_at, &session.created_at),
            monitor: false,
            subject: None,
            html_url: String::new(),
            pull_request: None,
        })
        .collect()
}

/// The list's order: what waits for the user first, then what runs, then
/// what finished; newest first inside each of those.
pub fn sort_rows(rows: &mut [AgentRow]) {
    rows.sort_by(|left, right| {
        let order = |row: &AgentRow| {
            StateGroup::ALL
                .iter()
                .position(|group| *group == row.group())
                .unwrap_or(StateGroup::ALL.len())
        };
        order(left)
            .cmp(&order(right))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.key.cmp(&right.key))
    });
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use subspace_framework::client_github::copilot_agents::{SubjectKind, Task};
    use subspace_framework::core::copilot_agent_tasks::{SubjectTaskRef, TaskRef};
    use subspace_framework::dashboard::chat_sessions::ChatSession;

    use super::{cloud_rows, local_rows, sort_rows};
    use crate::model::tests::row;
    use crate::model::{Kind, LOCAL_BUSY, LOCAL_OPEN, LOCAL_SAVED, StateGroup, UNKNOWN};

    fn task(id: &str, state: &str, updated: &str) -> Task {
        Task {
            id: id.into(),
            repo: "Owner/one".into(),
            name: format!("Fix {id}"),
            state: state.into(),
            html_url: format!("https://github.com/Owner/one/agents/tasks/{id}"),
            created_at: "2026-10-01T09:00:00Z".into(),
            updated_at: updated.into(),
            pull_request: None,
            subject: None,
        }
    }

    fn saved(subject: &str, id: &str, monitor: bool) -> SubjectTaskRef {
        SubjectTaskRef {
            subject: subject.into(),
            task: TaskRef {
                repo: "Owner/one".into(),
                task_id: id.into(),
                created_at: "2026-10-01T09:00:00Z".into(),
                monitor,
            },
        }
    }

    fn session(id: &str, open: bool, busy: bool) -> ChatSession {
        ChatSession {
            id: id.into(),
            title: format!("Chat {id}"),
            modified: "just now".into(),
            modified_at: "2026-10-06T09:00:00Z".into(),
            created_at: "2026-10-05T09:00:00Z".into(),
            open,
            busy,
        }
    }

    #[test]
    fn a_listed_task_takes_the_saved_monitor_flag_and_subject() {
        let tasks = [task("t1", "in_progress", "2026-10-02T09:00:00Z")];
        let saved = [saved("chat:abc", "t1", true)];
        let rows = cloud_rows(&tasks, &saved, &HashMap::new());
        assert_eq!(rows.len(), 1);
        assert!(rows[0].monitor);
        assert_eq!(rows[0].key, "cloud:t1");
        assert_eq!(
            rows[0].subject.as_ref().map(|s| s.kind),
            Some(SubjectKind::Chat)
        );
        assert!(rows[0].is_watched());
    }

    #[test]
    fn a_task_only_github_listed_is_not_monitored_by_default() {
        let tasks = [task("t1", "in_progress", "2026-10-02T09:00:00Z")];
        let rows = cloud_rows(&tasks, &[], &HashMap::new());
        assert!(!rows[0].monitor);
        assert!(!rows[0].is_watched());
        assert!(rows[0].subject.is_none());
    }

    #[test]
    fn a_saved_reference_the_list_left_out_still_gets_a_row() {
        let saved = [saved("chat:t9", "t9", true)];
        let rows = cloud_rows(&[], &saved, &HashMap::new());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, UNKNOWN);
        assert_eq!(rows[0].title, "Task t9");
        assert_eq!(rows[0].repo, "Owner/one");
        assert!(rows[0].monitor);
        assert!(
            !rows[0].is_watched(),
            "an unknown state is not polled as if it were running"
        );
    }

    #[test]
    fn a_loaded_task_replaces_what_the_list_and_the_file_knew() {
        let saved = [saved("chat:t9", "t9", true)];
        let known = HashMap::from([(
            "t9".to_owned(),
            task("t9", "completed", "2026-10-06T09:00:00Z"),
        )]);
        let rows = cloud_rows(&[], &saved, &known);
        assert_eq!(rows[0].state, "completed");
        assert_eq!(rows[0].title, "Fix t9");
        let stale = [task("t9", "queued", "2026-10-01T09:00:00Z")];
        let rows = cloud_rows(&stale, &saved, &known);
        assert_eq!(rows.len(), 1, "the row is not listed twice");
        assert_eq!(rows[0].state, "completed");
    }

    #[test]
    fn a_local_row_reports_whether_its_chat_tab_is_running() {
        let rows = local_rows(&[
            session("a", true, true),
            session("b", true, false),
            session("c", false, false),
        ]);
        assert_eq!(rows[0].state, LOCAL_BUSY);
        assert_eq!(rows[1].state, LOCAL_OPEN);
        assert_eq!(rows[2].state, LOCAL_SAVED);
        assert_eq!(rows[0].key, "local:a");
        assert!(rows[0].repo.is_empty());
        assert_eq!(rows[0].group(), StateGroup::Active);
    }

    #[test]
    fn the_list_puts_waiting_first_then_running_then_newest() {
        let mut rows = vec![
            row("done-old", Kind::Cloud, "completed", "2026-10-01T00:00:00Z"),
            row("run", Kind::Cloud, "in_progress", "2026-10-02T00:00:00Z"),
            row("done-new", Kind::Cloud, "completed", "2026-10-05T00:00:00Z"),
            row(
                "ask",
                Kind::Cloud,
                "waiting_for_user",
                "2026-10-01T00:00:00Z",
            ),
        ];
        sort_rows(&mut rows);
        let keys: Vec<&str> = rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys, ["ask", "run", "done-new", "done-old"]);
    }
}
