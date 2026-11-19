//! What one line of the Copilot Agents list is: its kind, its state and the keys and
//! subjects that identify it. No GPUI and no requests.

use subspace_framework::client_github::copilot_agents::{
    PullRequestArtifact, Subject, SubjectKind,
};
use subspace_framework::core::task_state::{is_active, needs_input};

/// The two kinds of row: a Copilot cloud task, and a local chat session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Cloud,
    Local,
}

impl Kind {
    pub const ALL: [Self; 2] = [Self::Cloud, Self::Local];

    pub fn id(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Local => "local",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cloud => "Cloud tasks",
            Self::Local => "Local chats",
        }
    }

    /// The mark the row carries at its left.
    pub fn mark(self) -> &'static str {
        match self {
            Self::Cloud => "\u{2601}",
            Self::Local => "\u{25b6}",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }
}

/// How the list groups a row's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateGroup {
    NeedsInput,
    Active,
    Finished,
}

impl StateGroup {
    /// In list order: what waits for the user comes first, then what runs.
    pub const ALL: [Self; 3] = [Self::NeedsInput, Self::Active, Self::Finished];

    pub fn id(self) -> &'static str {
        self.words().0
    }

    pub fn label(self) -> &'static str {
        self.words().1
    }

    /// The group's id and label, in one place per variant.
    // qual:allow(dry, repeated_matches) reason: "This enum has one mapping of its own; rustqual groups unrelated enums only because their three-arm variant matches share one shape."
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::NeedsInput => ("needs-input", "Needs input"),
            Self::Active => ("active", "Active"),
            Self::Finished => ("finished", "Finished"),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|group| group.id() == id)
    }
}

/// The state a local chat row reports. Local sessions have no cloud state,
/// so they get their own words rather than borrowing a task's.
pub const LOCAL_BUSY: &str = "busy";
pub const LOCAL_OPEN: &str = "open";
pub const LOCAL_SAVED: &str = "saved";
/// A cloud task that only a saved reference knows about, until a detail
/// request or a poll fills it in.
pub const UNKNOWN: &str = "unknown";

/// Which group a state belongs to. A local session that a chat tab is
/// running counts as active; every other local session is finished work.
pub fn state_group(state: &str) -> StateGroup {
    if needs_input(state) {
        StateGroup::NeedsInput
    } else if is_active(state) || state == LOCAL_BUSY {
        StateGroup::Active
    } else {
        StateGroup::Finished
    }
}

/// A state in the words the list shows: `in_progress` reads "in progress".
pub fn state_label(state: &str) -> String {
    state.replace('_', " ")
}

/// One line of the Copilot Agents list.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    /// Unique in the list: `cloud:<task id>` or `local:<session id>`.
    pub key: String,
    pub kind: Kind,
    /// The task id or the session id, without the kind prefix.
    pub id: String,
    pub title: String,
    /// `owner/name`, empty for a local session.
    pub repo: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    /// Whether a finished or stuck task raises an alert.
    pub monitor: bool,
    /// The chat that started the task.
    pub subject: Option<Subject>,
    /// The task's page on GitHub, empty for a local session.
    pub html_url: String,
    pub pull_request: Option<PullRequestArtifact>,
}

impl AgentRow {
    pub fn group(&self) -> StateGroup {
        state_group(&self.state)
    }

    /// Whether a watcher should poll this row.
    pub fn is_watched(&self) -> bool {
        self.kind == Kind::Cloud && self.monitor && is_active(&self.state)
    }
}

/// The key a cloud task's row carries.
pub fn cloud_key(task_id: &str) -> String {
    format!("cloud:{task_id}")
}

/// The key a local session's row carries.
pub fn local_key(session: &str) -> String {
    format!("local:{session}")
}

/// The stored subject string `<kind>:<id>` as a [`Subject`]. An unknown kind
/// reads as [`SubjectKind::Other`], so a newer file still opens.
pub fn parse_subject(stored: &str) -> Option<Subject> {
    let (kind, id) = stored.split_once(':')?;
    let kind = match kind {
        "chat" => SubjectKind::Chat,
        _ => SubjectKind::Other,
    };
    Some(Subject {
        kind,
        id: id.to_owned(),
    })
}

/// The stored subject string a [`Subject`] writes back as.
pub fn subject_key(subject: &Subject) -> String {
    let kind = match subject.kind {
        SubjectKind::Chat => "chat",
        SubjectKind::Other => "other",
    };
    format!("{kind}:{}", subject.id)
}

#[cfg(test)]
pub(crate) mod tests {
    use subspace_framework::client_github::copilot_agents::{Subject, SubjectKind};

    use super::{AgentRow, Kind, StateGroup, parse_subject, state_group, state_label, subject_key};

    pub(crate) fn row(key: &str, kind: Kind, state: &str, updated: &str) -> AgentRow {
        AgentRow {
            key: key.into(),
            kind,
            id: key.into(),
            title: key.into(),
            repo: "Owner/one".into(),
            state: state.into(),
            created_at: "2026-10-01T09:00:00Z".into(),
            updated_at: updated.into(),
            monitor: false,
            subject: None,
            html_url: String::new(),
            pull_request: None,
        }
    }

    #[test]
    fn states_group_by_what_the_user_must_do_about_them() {
        assert_eq!(state_group("waiting_for_user"), StateGroup::NeedsInput);
        assert_eq!(state_group("in_progress"), StateGroup::Active);
        assert_eq!(state_group("queued"), StateGroup::Active);
        assert_eq!(state_group("busy"), StateGroup::Active);
        assert_eq!(state_group("completed"), StateGroup::Finished);
        assert_eq!(state_group("saved"), StateGroup::Finished);
        assert_eq!(
            state_group("new_state"),
            StateGroup::Finished,
            "an unknown state is not claimed to be running"
        );
        assert_eq!(state_label("in_progress"), "in progress");
    }

    #[test]
    fn only_a_monitored_running_cloud_task_is_watched() {
        let mut watched = row("cloud:a", Kind::Cloud, "in_progress", "");
        assert!(!watched.is_watched(), "an unmonitored task is not polled");
        watched.monitor = true;
        assert!(watched.is_watched());
        watched.state = "completed".into();
        assert!(!watched.is_watched());
        let mut local = row("local:s", Kind::Local, "busy", "");
        local.monitor = true;
        assert!(!local.is_watched(), "a local chat is never polled");
        assert_eq!(local.group(), StateGroup::Active);
    }

    #[test]
    fn row_keys_name_their_kind() {
        assert_eq!(super::cloud_key("t1"), "cloud:t1");
        assert_eq!(super::local_key("s1"), "local:s1");
    }

    #[test]
    fn subject_strings_round_trip_and_an_unknown_kind_reads_as_other() {
        for stored in ["chat:c", "other:o"] {
            let subject = parse_subject(stored).unwrap();
            assert_eq!(subject_key(&subject), stored);
        }
        assert_eq!(
            parse_subject("future-kind:x").map(|s| s.kind),
            Some(SubjectKind::Other)
        );
        assert!(parse_subject("nocolon").is_none());
        assert_eq!(
            subject_key(&Subject {
                kind: SubjectKind::Chat,
                id: "t1".into()
            }),
            "chat:t1"
        );
    }

    #[test]
    fn kinds_and_groups_round_trip_through_their_stored_ids() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_id(kind.id()), Some(kind));
            assert!(!kind.label().is_empty() && !kind.mark().is_empty());
        }
        for group in StateGroup::ALL {
            assert_eq!(StateGroup::from_id(group.id()), Some(group));
            assert!(!group.label().is_empty());
        }
        assert_eq!(Kind::from_id("other"), None);
        assert_eq!(StateGroup::from_id("other"), None);
    }
}
