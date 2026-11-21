//! What the opened tab shows for a cloud task and for a local chat.

use subspace_framework::client_github::copilot_agents::{
    TaskDetail, TaskSession, session_browser_url,
};
use subspace_framework::dashboard::{Detail, Section};

use crate::dates;
use crate::model::{AgentRow, Kind, UNKNOWN, state_label, subject_key};

/// What the panel says while a task's detail is still loading or failed.
pub const LOADING: &str = "Loading the task\u{2026}";

/// A label and value pair, left out when the value is empty.
fn prop(label: &str, value: String) -> Option<(gpui_kit::SharedString, gpui_kit::SharedString)> {
    (!value.is_empty()).then(|| (label.to_owned().into(), value.into()))
}

/// The properties every row shows, from the row alone.
fn row_props(row: &AgentRow) -> Vec<(gpui_kit::SharedString, gpui_kit::SharedString)> {
    let monitor = match row.kind {
        Kind::Cloud => Some(if row.monitor { "on" } else { "off" }.to_owned()),
        Kind::Local => None,
    };
    [
        prop("State", state_label(&row.state)),
        prop("Repository", row.repo.clone()),
        prop("Created", dates::stamp(&row.created_at)),
        prop("Updated", dates::stamp(&row.updated_at)),
        prop("Monitor", monitor.unwrap_or_default()),
        prop(
            "Subject",
            row.subject.as_ref().map(subject_key).unwrap_or_default(),
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The single clickable pull request reference, when its URL is safe.
fn pull_section(row: &AgentRow) -> Option<Section> {
    let pull = row.pull_request.as_ref()?;
    let url = subspace_framework::dashboard::links::safe_url(&pull.url)?;
    Some(Section::markdown(
        "Pull request",
        format!(
            "[#{number} ({state})]({url})",
            number = pull.number,
            state = crate::rows::pull_label(pull.state)
        ),
    ))
}

/// One agent run inside the task, as a panel section. The prompt is shown as
/// Markdown, which is what the agent was given.
fn session_section(index: usize, session: &TaskSession, url: Option<String>) -> Section {
    let mut lines = vec![format!(
        "**State** {}  ·  **Model** {}",
        state_label(&session.state),
        if session.model.is_empty() {
            "unknown"
        } else {
            &session.model
        }
    )];
    if let Some(url) = url {
        lines.push(format!("[Open this session in browser]({url})"));
    }
    if !session.base_ref.is_empty() || !session.head_ref.is_empty() {
        lines.push(format!(
            "**Branch** `{}` \u{2192} `{}`",
            session.base_ref, session.head_ref
        ));
    }
    lines.push(format!(
        "**Started** {}  ·  **Updated** {}{}",
        dates::stamp(&session.created_at),
        dates::stamp(&session.updated_at),
        session
            .completed_at
            .as_deref()
            .map(|at| format!("  ·  **Finished** {}", dates::stamp(at)))
            .unwrap_or_default()
    ));
    if let Some(usage) = &session.usage {
        lines.push(format!(
            "**Usage** {} {}",
            usage.amount,
            state_label(&usage.kind)
        ));
    }
    if let Some(error) = &session.error {
        lines.push(format!("**Error** {}", error.message));
    }
    if !session.prompt.is_empty() {
        lines.push(format!("\n**Prompt**\n\n{}", session.prompt));
    }
    Section::markdown(session_heading(index, session), lines.join("\n\n"))
}

fn session_heading(index: usize, session: &TaskSession) -> String {
    match session.name.trim() {
        "" => format!("Session {}", index + 1),
        name => format!("Session {}: {name}", index + 1),
    }
}

/// The sections a loaded task adds: one per agent run, newest last as the
/// API orders them.
pub fn session_sections(detail: &TaskDetail) -> Vec<Section> {
    detail
        .sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let url = detail
                .task
                .pull_request
                .as_ref()
                .and_then(|pull| session_browser_url(&pull.url, Some(&session.id)));
            session_section(index, session, url)
        })
        .collect()
}

/// The panel of a cloud task. `sections` are the loaded sessions, or a note
/// while they load.
pub fn cloud_detail(row: &AgentRow, sections: Vec<Section>) -> Detail {
    let props = row_props(row);
    let mut sections = sections;
    if let Some(pull) = pull_section(row) {
        sections.insert(0, pull);
    }
    Detail {
        kind: "Agent task".into(),
        id: row.id.clone().into(),
        title: row.title.clone().into(),
        link: crate::browser::link(row).map(|mut link| {
            link.label = if link.url
                == subspace_framework::client_github::copilot_agents::COPILOT_AGENTS_HOME_URL
            {
                "GitHub Agents (o)".into()
            } else {
                "Browser (o)".into()
            };
            link
        }),
        props,
        sections,
    }
}

/// The panel of a local chat session.
pub fn local_detail(row: &AgentRow) -> Detail {
    Detail {
        kind: "Chat session".into(),
        id: row.id.clone().into(),
        title: row.title.clone().into(),
        link: None,
        props: row_props(row),
        sections: vec![Section::text(
            "Chat",
            "Open in chat (c) shows this session in a chat tab.",
        )],
    }
}

/// The note while sessions load. Failures use the shared actionable error UI.
pub fn note_section(state: &str) -> Section {
    match state {
        UNKNOWN => Section::text(
            "Sessions",
            "This task comes from a saved reference. Its state loads when it is selected or opened.",
        ),
        _ => Section::text("Sessions", LOADING),
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::client_github::copilot_agents::{
        PullRequestArtifact, SessionError, SessionUsage, Subject, SubjectKind, Task, TaskDetail,
        TaskSession,
    };
    use subspace_framework::client_github::model::PrState;

    use super::{cloud_detail, local_detail, note_section, session_sections};
    use crate::model::{AgentRow, Kind, UNKNOWN};

    fn row() -> AgentRow {
        AgentRow {
            key: "cloud:t1".into(),
            kind: Kind::Cloud,
            id: "t1".into(),
            title: "Fix the login button".into(),
            repo: "Owner/one".into(),
            state: "in_progress".into(),
            created_at: "2026-10-01T09:00:00Z".into(),
            updated_at: "2026-10-02T09:00:00Z".into(),
            monitor: true,
            subject: Some(Subject {
                kind: SubjectKind::Chat,
                id: "abc".into(),
            }),
            html_url: "https://github.com/Owner/one/agents/tasks/t1".into(),
            pull_request: Some(PullRequestArtifact {
                number: 7,
                url: "https://github.com/Owner/one/pull/7".into(),
                state: PrState::Open,
            }),
        }
    }

    fn props(detail: &subspace_framework::dashboard::Detail) -> Vec<(String, String)> {
        detail
            .props
            .iter()
            .map(|(label, value)| (label.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn a_cloud_panel_shows_the_state_times_monitor_subject_and_pull_request() {
        let detail = cloud_detail(&row(), Vec::new());
        let props = props(&detail);
        assert_eq!(detail.kind, "Agent task");
        assert_eq!(detail.id, "t1");
        assert_eq!(
            detail.link.as_ref().map(|link| link.url.to_string()),
            Some("https://github.com/Owner/one/agents/tasks/t1".to_owned())
        );
        assert!(props.contains(&("State".into(), "in progress".into())));
        assert!(props.contains(&("Repository".into(), "Owner/one".into())));
        assert!(props.contains(&("Monitor".into(), "on".into())));
        assert!(props.contains(&("Subject".into(), "chat:abc".into())));
        assert!(detail.sections.iter().any(|section| {
            matches!(&section.body, subspace_framework::dashboard::Body::Markdown(text)
                if text == "[#7 (open)](https://github.com/Owner/one/pull/7)")
        }));
        assert!(props.iter().all(|(label, _)| label != "Pull request"));
        assert!(props.iter().all(|(label, _)| label != "Pull request link"));
        assert!(props.iter().any(|(label, _)| label == "Created"));
    }

    #[test]
    fn a_task_without_a_focused_page_offers_the_github_agents_home_page() {
        let mut plain = row();
        plain.html_url = String::new();
        plain.pull_request = None;
        plain.subject = None;
        plain.monitor = false;
        let detail = cloud_detail(&plain, Vec::new());
        let link = detail.link.as_ref().unwrap();
        assert_eq!(link.url, "https://github.com/copilot/agents");
        assert!(link.label.contains("GitHub Agents"));
        let props = props(&detail);
        assert!(props.iter().all(|(label, _)| label != "Pull request"));
        assert!(props.iter().all(|(label, _)| label != "Subject"));
        assert!(props.contains(&("Monitor".into(), "off".into())));
    }

    #[test]
    fn an_unsafe_pull_request_url_is_not_rendered_as_markdown() {
        let mut unsafe_row = row();
        unsafe_row.pull_request.as_mut().unwrap().url = "javascript:alert(1)".into();
        let detail = cloud_detail(&unsafe_row, Vec::new());
        assert!(detail.sections.is_empty());
        assert!(
            props(&detail)
                .iter()
                .all(|(label, _)| label != "Pull request" && label != "Pull request link")
        );
    }

    #[test]
    fn a_local_panel_shows_times_but_no_repository_monitor_or_link() {
        let mut local = row();
        local.kind = Kind::Local;
        local.repo = String::new();
        local.subject = None;
        local.state = "open".into();
        let detail = local_detail(&local);
        let props = props(&detail);
        assert_eq!(detail.kind, "Chat session");
        assert!(detail.link.is_none());
        assert!(props.contains(&("State".into(), "open".into())));
        assert!(props.iter().all(|(label, _)| label != "Repository"));
        assert!(props.iter().all(|(label, _)| label != "Monitor"));
        assert_eq!(detail.sections.len(), 1);
    }

    fn session(name: &str) -> TaskSession {
        TaskSession {
            id: "s1".into(),
            name: name.into(),
            state: "failed".into(),
            created_at: "2026-10-01T09:00:00Z".into(),
            updated_at: "2026-10-01T09:30:00Z".into(),
            completed_at: Some("2026-10-01T09:30:00Z".into()),
            prompt: "fix it".into(),
            head_ref: "copilot/fix-1".into(),
            base_ref: "main".into(),
            model: "gpt-5.4".into(),
            usage: Some(SessionUsage {
                kind: "premium_requests".into(),
                amount: 1.5,
            }),
            error: Some(SessionError {
                message: "the build failed".into(),
            }),
        }
    }

    fn bare_session() -> TaskSession {
        TaskSession {
            id: "s2".into(),
            name: String::new(),
            state: "completed".into(),
            created_at: String::new(),
            updated_at: String::new(),
            completed_at: None,
            prompt: String::new(),
            head_ref: String::new(),
            base_ref: String::new(),
            model: String::new(),
            usage: None,
            error: None,
        }
    }

    fn detail(sessions: Vec<TaskSession>) -> TaskDetail {
        TaskDetail {
            task: Task {
                id: "t1".into(),
                repo: "Owner/one".into(),
                name: "Fix".into(),
                state: "completed".into(),
                html_url: String::new(),
                created_at: String::new(),
                updated_at: String::new(),
                pull_request: None,
                subject: None,
            },
            sessions,
        }
    }

    fn markdown(section: &subspace_framework::dashboard::Section) -> String {
        match &section.body {
            subspace_framework::dashboard::Body::Markdown(text) => text.to_string(),
            other => panic!("a session section is Markdown, not {other:?}"),
        }
    }

    #[test]
    fn a_session_section_shows_its_run_its_branches_and_its_prompt() {
        let sections = session_sections(&detail(vec![session("first attempt")]));
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].heading, "Session 1: first attempt");
        let body = markdown(&sections[0]);
        assert!(body.contains("**State** failed"));
        assert!(body.contains("gpt-5.4"));
        assert!(body.contains("`main` \u{2192} `copilot/fix-1`"));
        assert!(body.contains("premium requests"));
        assert!(body.contains("the build failed"));
        assert!(body.contains("fix it"));
    }

    #[test]
    fn each_run_links_to_its_own_supported_browser_route() {
        let mut loaded = detail(vec![session("first"), bare_session()]);
        loaded.task.pull_request = row().pull_request;
        let sections = session_sections(&loaded);
        assert!(
            markdown(&sections[0])
                .contains("https://github.com/Owner/one/pull/7/agent-sessions/s1")
        );
        assert!(
            markdown(&sections[1])
                .contains("https://github.com/Owner/one/pull/7/agent-sessions/s2")
        );
    }

    #[test]
    fn a_session_with_nothing_recorded_is_numbered_and_leaves_the_gaps_out() {
        let sections = session_sections(&detail(vec![session("a"), bare_session()]));
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[1].heading, "Session 2");
        let body = markdown(&sections[1]);
        assert!(body.contains("unknown"), "a missing model is named as such");
        assert!(!body.contains("**Error**"));
        assert!(!body.contains("**Prompt**"));
        assert!(!body.contains("**Usage**"));
        assert!(session_sections(&detail(Vec::new())).is_empty());
    }

    #[test]
    fn the_note_tells_loading_and_a_reference_only_task_apart() {
        assert!(matches!(
            &note_section("in_progress").body,
            subspace_framework::dashboard::Body::Text(text) if text.contains("Loading")
        ));
        assert!(matches!(
            &note_section(UNKNOWN).body,
            subspace_framework::dashboard::Body::Text(text) if text.contains("saved reference")
        ));
    }
}
