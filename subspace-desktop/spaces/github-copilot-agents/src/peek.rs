//! What the inspector, the list's lanes and the opened tab say about one
//! row: its status in words with a glyph and a tone, a short name, an
//! excerpt, its facts and the actions its footer offers. No GPUI and no
//! requests.

use chrono::{DateTime, Utc};
use subspace_framework::client_github::copilot_agents::{TaskDetail, TaskSession};
use subspace_framework::client_github::model::PrState;
use subspace_framework::dashboard::Tone;
use subspace_framework::lucide::{IconName, SvgIcon};

use crate::dates;
use crate::logs::clip;
use crate::model::{AgentRow, Kind, LOCAL_BUSY, LOCAL_OPEN, UNKNOWN, state_label, subject_key};
use crate::rows::{pull_label, pull_tone, state_tone};

/// The most of a session's prompt the inspector shows.
const EXCERPT_CHARS: usize = 240;
/// The most of a title the opened item's tab shows.
const TAB_TITLE_CHARS: usize = 40;

/// What the inspector says with nothing selected.
pub(crate) const NOTHING_SELECTED: &str = "Select a task to see it here.";
/// What a local chat's place reads as, where a cloud task names its
/// repository.
pub(crate) const LOCAL_PLACE: &str = "Local chat";

/// A state's glyph. Each state has its own shape, so it reads without its
/// colour.
pub(crate) fn state_icon(state: &str) -> SvgIcon {
    match state {
        "waiting_for_user" => IconName::CircleQuestionMark,
        "failed" | "timed_out" => IconName::CircleX,
        "cancelled" => IconName::CircleSlash,
        "completed" => IconName::CircleCheck,
        "queued" => IconName::Clock,
        "in_progress" | LOCAL_BUSY => IconName::LoaderCircle,
        LOCAL_OPEN => IconName::CircleDot,
        "idle" | UNKNOWN => IconName::CircleDashed,
        _ => IconName::Circle,
    }
}

/// Every pull request state, with its glyph: one shape per state.
const PULL_ICONS: [(PrState, SvgIcon); 4] = [
    (PrState::Open, IconName::GitPullRequest),
    (PrState::Draft, IconName::GitPullRequestDraft),
    (PrState::Merged, IconName::GitMerge),
    (PrState::Closed, IconName::GitPullRequestClosed),
];

/// A pull request state's glyph, one shape per state.
pub(crate) fn pull_icon(state: PrState) -> SvgIcon {
    PULL_ICONS
        .iter()
        .find(|(known, _)| *known == state)
        .map_or(IconName::GitPullRequest, |(_, icon)| *icon)
}

/// A status in words, with the glyph and the tone it shows in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Status {
    pub(crate) words: String,
    pub(crate) icon: SvgIcon,
    pub(crate) tone: Tone,
}

/// The status a row's state reads as.
pub(crate) fn status(state: &str) -> Status {
    Status {
        words: state_label(state),
        icon: state_icon(state),
        tone: state_tone(state),
    }
}

/// A task's pull request, as the inspector's row and the list's lane show it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pull {
    pub(crate) number: u32,
    pub(crate) status: Status,
}

impl Pull {
    /// What the row reads as, such as `#7 open`.
    pub(crate) fn label(&self) -> String {
        format!("#{} {}", self.number, self.status.words)
    }
}

pub(crate) fn pull(row: &AgentRow) -> Option<Pull> {
    row.pull_request.as_ref().map(|pull| Pull {
        number: pull.number,
        status: Status {
            words: pull_label(pull.state).to_owned(),
            icon: pull_icon(pull.state),
            tone: pull_tone(pull.state),
        },
    })
}

/// What the inspector shows of one row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Peek {
    /// The header's brief name, such as `one #7`.
    pub(crate) name: String,
    pub(crate) status: Status,
    pub(crate) title: String,
    /// The start of the last session's prompt, once the sessions loaded.
    pub(crate) excerpt: Option<String>,
    /// Label and value pairs, the empty ones left out.
    pub(crate) facts: Vec<(&'static str, String)>,
    pub(crate) pull: Option<Pull>,
}

/// The inspector's view of `row`. `detail` is its loaded sessions, when
/// the one detail slot holds this row's.
pub(crate) fn peek(row: &AgentRow, detail: Option<&TaskDetail>) -> Peek {
    let session = detail.and_then(|detail| detail.sessions.last());
    Peek {
        name: name(row),
        status: status(&row.state),
        title: row.title.clone(),
        excerpt: session.and_then(excerpt),
        facts: facts(row, session),
        pull: pull(row),
    }
}

/// The header's brief name: the repository's short name and the pull
/// request, or the kind of row.
pub(crate) fn name(row: &AgentRow) -> String {
    if row.kind == Kind::Local {
        return "Chat session".to_owned();
    }
    let repo = row.repo.rsplit('/').next().unwrap_or_default().trim();
    let pull = row
        .pull_request
        .as_ref()
        .map(|pull| format!("#{}", pull.number));
    match (repo.is_empty(), pull) {
        (false, Some(pull)) => format!("{repo} {pull}"),
        (false, None) => repo.to_owned(),
        (true, Some(pull)) => format!("Agent task {pull}"),
        (true, None) => "Agent task".to_owned(),
    }
}

/// Where a row belongs: its repository, or that it is a local chat.
pub(crate) fn place(row: &AgentRow) -> String {
    match row.kind {
        Kind::Local => LOCAL_PLACE.to_owned(),
        Kind::Cloud => row.repo.clone(),
    }
}

/// The opened item's tab label: its title, cut short.
pub(crate) fn tab_title(row: &AgentRow) -> String {
    match row.title.trim() {
        "" => name(row),
        title => clip(title, TAB_TITLE_CHARS),
    }
}

/// The facts the old detail panel's properties and its last session
/// showed, the empty ones left out.
fn facts(row: &AgentRow, session: Option<&TaskSession>) -> Vec<(&'static str, String)> {
    let monitor = match row.kind {
        Kind::Cloud => on_off(row.monitor).to_owned(),
        Kind::Local => String::new(),
    };
    [
        ("Repository", row.repo.clone()),
        ("Branch", session.map(branch).unwrap_or_default()),
        (
            "Model",
            session
                .map(|session| session.model.clone())
                .unwrap_or_default(),
        ),
        ("Started", dates::stamp(&row.created_at)),
        ("Updated", dates::stamp(&row.updated_at)),
        ("Monitor", monitor),
        (
            "Subject",
            row.subject.as_ref().map(subject_key).unwrap_or_default(),
        ),
    ]
    .into_iter()
    .filter(|(_, value)| !value.trim().is_empty())
    .collect()
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// The session's branch, then the one it merges into.
fn branch(session: &TaskSession) -> String {
    match (session.head_ref.trim(), session.base_ref.trim()) {
        ("", base) => base.to_owned(),
        (head, "") => head.to_owned(),
        (head, base) => format!("{head} \u{2192} {base}"),
    }
}

/// The first paragraph of the session's prompt, on one line and cut short.
fn excerpt(session: &TaskSession) -> Option<String> {
    let paragraph = session
        .prompt
        .split("\n\n")
        .map(str::trim)
        .find(|paragraph| !paragraph.is_empty())?;
    let line = paragraph.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(clip(&line, EXCERPT_CHARS))
}

/// A row's accessible name: the title, then its state, place, pull request,
/// whether it is monitored and how long ago it changed.
pub(crate) fn row_label(row: &AgentRow, now: DateTime<Utc>) -> String {
    let age = dates::age(&row.updated_at, now);
    [
        row.title.clone(),
        state_label(&row.state),
        place(row),
        pull(row)
            .map(|pull| format!("pull request {}", pull.label()))
            .unwrap_or_default(),
        (if row.monitor { "monitored" } else { "" }).to_owned(),
        if age.is_empty() {
            age
        } else {
            format!("updated {age} ago")
        },
    ]
    .into_iter()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>()
    .join(", ")
}

/// One action the inspector's footer offers on its row, each the same as
/// the key and the palette command that do it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Act {
    /// Open in chat (c).
    Chat,
    /// Show the session log, or hide it (l).
    Log { shown: bool },
    /// Monitor the task, or stop (m).
    Monitor { on: bool },
    /// Copy link (y).
    CopyLink,
    /// Name the chat the task came from (u).
    Subject,
}

impl Act {
    /// The button's id, label and glyph.
    pub(crate) fn words(self) -> (&'static str, &'static str, SvgIcon) {
        match self {
            Self::Chat => (
                "github-copilot-agents-act-chat",
                "Open in chat",
                IconName::MessageSquare,
            ),
            Self::Log { shown } => (
                "github-copilot-agents-act-log",
                if shown { "Hide log" } else { "Session log" },
                IconName::ScrollText,
            ),
            Self::Monitor { on } => (
                "github-copilot-agents-act-monitor",
                if on { "Stop monitoring" } else { "Monitor" },
                IconName::Bell,
            ),
            Self::CopyLink => (
                "github-copilot-agents-act-copy",
                "Copy link",
                IconName::Copy,
            ),
            Self::Subject => (
                "github-copilot-agents-act-subject",
                "Show subject",
                IconName::AtSign,
            ),
        }
    }
}

/// The footer's actions for `row`: a local chat opens in chat; a cloud
/// task also has its log, monitoring, its link and, when a chat started
/// it, its subject. `log_shown` says whether this task's log is on show.
pub(crate) fn acts(row: &AgentRow, log_shown: bool) -> Vec<Act> {
    match row.kind {
        Kind::Local => vec![Act::Chat],
        Kind::Cloud => {
            let mut acts = vec![
                Act::Chat,
                Act::Log { shown: log_shown },
                Act::Monitor { on: row.monitor },
                Act::CopyLink,
            ];
            acts.extend(row.subject.is_some().then_some(Act::Subject));
            acts
        }
    }
}

#[cfg(test)]
mod tests;
