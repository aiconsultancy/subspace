use chrono::{DateTime, Utc};
use subspace_framework::client_github::copilot_agents::{
    PullRequestArtifact, Subject, SubjectKind, Task, TaskDetail, TaskSession,
};
use subspace_framework::client_github::model::PrState;
use subspace_framework::dashboard::Tone;
use subspace_framework::lucide::IconName;

use super::{
    Act, NOTHING_SELECTED, acts, name, peek, place, pull_icon, row_label, state_icon, status,
    tab_title,
};
use crate::model::tests::row;
use crate::model::{AgentRow, Kind};

fn cloud() -> AgentRow {
    let mut cloud = row(
        "cloud:t1",
        Kind::Cloud,
        "in_progress",
        "2026-10-07T09:00:00Z",
    );
    cloud.title = "Fix the login button".into();
    cloud.pull_request = Some(PullRequestArtifact {
        number: 7,
        url: "https://github.com/Owner/one/pull/7".into(),
        state: PrState::Draft,
    });
    cloud
}

fn session(prompt: &str) -> TaskSession {
    TaskSession {
        id: "s1".into(),
        name: String::new(),
        state: "in_progress".into(),
        created_at: String::new(),
        updated_at: String::new(),
        completed_at: None,
        prompt: prompt.into(),
        head_ref: "copilot/fix-1".into(),
        base_ref: "main".into(),
        model: "gpt-5.4".into(),
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
            state: "in_progress".into(),
            html_url: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            pull_request: None,
            subject: None,
        },
        sessions,
    }
}

fn labels(facts: &[(&'static str, String)]) -> Vec<&'static str> {
    facts.iter().map(|(label, _)| *label).collect()
}

#[test]
fn each_state_has_its_own_glyph_so_it_reads_without_colour() {
    let states = [
        "waiting_for_user",
        "failed",
        "cancelled",
        "completed",
        "queued",
        "in_progress",
        "open",
        "unknown",
    ];
    let icons: Vec<_> = states.iter().map(|state| state_icon(state)).collect();
    for (index, icon) in icons.iter().enumerate() {
        assert!(
            !icons[index + 1..].contains(icon),
            "{} shares its glyph",
            states[index]
        );
    }
    assert_eq!(state_icon("busy"), state_icon("in_progress"));
    assert_eq!(state_icon("new_state"), IconName::Circle);
    assert_ne!(pull_icon(PrState::Open), pull_icon(PrState::Merged));
    let failed = status("failed");
    assert_eq!(failed.words, "failed");
    assert_eq!(failed.tone, Tone::Red);
}

#[test]
fn the_header_names_the_item_briefly() {
    assert_eq!(name(&cloud()), "one #7");
    let mut bare = cloud();
    bare.pull_request = None;
    assert_eq!(name(&bare), "one");
    bare.repo = String::new();
    assert_eq!(name(&bare), "Agent task");
    let mut local = cloud();
    local.kind = Kind::Local;
    assert_eq!(name(&local), "Chat session");
    assert_eq!(place(&local), "Local chat");
    assert_eq!(place(&cloud()), "Owner/one");
}

#[test]
fn a_selected_task_shows_its_row_facts_until_its_sessions_load() {
    let mut task = cloud();
    task.subject = Some(Subject {
        kind: SubjectKind::Chat,
        id: "abc".into(),
    });
    let early = peek(&task, None);
    assert_eq!(early.status.words, "in progress");
    assert_eq!(early.title, "Fix the login button");
    assert!(early.excerpt.is_none());
    assert_eq!(
        labels(&early.facts),
        ["Repository", "Started", "Updated", "Monitor", "Subject"]
    );
    assert_eq!(
        early.pull.as_ref().map(|pull| pull.label()).as_deref(),
        Some("#7 draft")
    );

    let loaded = detail(vec![session("Fix the login button.\n\nThen add a test.")]);
    let late = peek(&task, Some(&loaded));
    assert_eq!(
        labels(&late.facts),
        [
            "Repository",
            "Branch",
            "Model",
            "Started",
            "Updated",
            "Monitor",
            "Subject"
        ]
    );
    assert!(
        late.facts
            .contains(&("Branch", "copilot/fix-1 \u{2192} main".into()))
    );
    assert_eq!(late.excerpt.as_deref(), Some("Fix the login button."));
}

#[test]
fn a_long_prompt_is_cut_to_a_short_excerpt_and_a_long_title_to_a_short_tab() {
    let long = "word ".repeat(200);
    let excerpt = peek(&cloud(), Some(&detail(vec![session(&long)])))
        .excerpt
        .unwrap_or_default();
    assert!(excerpt.ends_with('\u{2026}'));
    assert!(excerpt.chars().count() <= super::EXCERPT_CHARS + 1);
    let mut task = cloud();
    task.title = "t".repeat(100);
    assert_eq!(tab_title(&task).chars().count(), super::TAB_TITLE_CHARS + 1);
    task.title = "  ".into();
    assert_eq!(tab_title(&task), "one #7");
}

#[test]
fn a_local_chat_shows_no_repository_monitor_or_pull_request() {
    let mut local = row("local:s", Kind::Local, "saved", "");
    local.repo = String::new();
    let shown = peek(&local, None);
    assert_eq!(labels(&shown.facts), ["Started"]);
    assert!(shown.pull.is_none());
    assert!(!NOTHING_SELECTED.is_empty());
}

#[test]
fn a_row_is_named_with_its_state_place_pull_request_and_age() {
    let now: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-10-07T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut task = cloud();
    task.monitor = true;
    assert_eq!(
        row_label(&task, now),
        "Fix the login button, in progress, Owner/one, pull request #7 draft, monitored, updated 3h ago"
    );
}

#[test]
fn the_footer_offers_what_the_keys_and_palette_do_for_that_kind_of_row() {
    let mut local = cloud();
    local.kind = Kind::Local;
    assert_eq!(acts(&local, false), [Act::Chat]);
    assert_eq!(
        acts(&cloud(), true),
        [
            Act::Chat,
            Act::Log { shown: true },
            Act::Monitor { on: false },
            Act::CopyLink
        ]
    );
    let mut started = cloud();
    started.subject = Some(Subject {
        kind: SubjectKind::Chat,
        id: "abc".into(),
    });
    assert_eq!(acts(&started, false).last(), Some(&Act::Subject));
    assert_eq!(Act::Log { shown: true }.words().1, "Hide log");
    assert_eq!(Act::Monitor { on: true }.words().1, "Stop monitoring");
}
