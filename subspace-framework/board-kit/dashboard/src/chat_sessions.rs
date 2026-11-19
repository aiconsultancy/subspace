//! What the chat panel publishes about its saved sessions, so a dashboard
//! can list them without owning the runtime.
//!
//! The host's chat panel holds its
//! `AgentHandle`; a dashboard only reads this snapshot from a GPUI global
//! and asks the shell to act with
//! [`crate::ChatRequest`]. Nothing here starts a session or sends a message.

use gpui_kit::{App, Global};
use subspace_agent_model::SessionSummary;

/// One saved chat, with what the panel knows about its tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSession {
    pub id: String,
    /// The session name, else a title cleaned from its first message.
    pub title: String,
    /// How long ago it changed, ready to show, such as "5 min ago".
    pub modified: String,
    /// ISO 8601 modification time, as the SDK reported it.
    pub modified_at: String,
    /// ISO 8601 creation time, as the SDK reported it.
    pub created_at: String,
    /// A chat tab holds this session.
    pub open: bool,
    /// That tab is running a turn.
    pub busy: bool,
}

/// The panel's latest session list. Absent until the runtime answers.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ChatSessions(pub Vec<ChatSession>);

impl Global for ChatSessions {}

/// The saved chats the panel last published, newest first as the runtime
/// ordered them. Empty before the runtime starts.
pub fn sessions(cx: &App) -> Vec<ChatSession> {
    cx.try_global::<ChatSessions>()
        .map(|published| published.0.clone())
        .unwrap_or_default()
}

/// Publishes `sessions`, leaving the global alone when nothing changed, so
/// readers that observe it are not woken for an identical list.
pub fn publish(cx: &mut App, sessions: Vec<ChatSession>) {
    if cx.try_global::<ChatSessions>().map(|held| &held.0) != Some(&sessions) {
        cx.set_global(ChatSessions(sessions));
    }
}

/// Pairs each saved session with its open tab, when a tab holds it. `open`
/// is each tab's session id and whether that tab is running a turn.
pub fn with_tabs(summaries: &[SessionSummary], open: &[(String, bool)]) -> Vec<ChatSession> {
    summaries
        .iter()
        .map(|summary| {
            let tab = open.iter().find(|(id, _)| *id == summary.id);
            ChatSession {
                id: summary.id.clone(),
                title: summary.title.clone(),
                modified: summary.modified.clone(),
                modified_at: summary.modified_at.clone(),
                created_at: summary.created_at.clone(),
                open: tab.is_some(),
                busy: tab.is_some_and(|(_, busy)| *busy),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ChatSessions, publish, sessions, with_tabs};
    use gpui_kit::TestAppContext;
    use std::cell::Cell;
    use std::rc::Rc;
    use subspace_agent_model::SessionSummary;

    fn summary(id: &str) -> SessionSummary {
        SessionSummary {
            id: id.into(),
            title: format!("Chat {id}"),
            modified: "just now".into(),
            modified_at: "2026-10-07T09:00:00Z".into(),
            created_at: "2026-10-07T08:00:00Z".into(),
        }
    }

    #[test]
    fn a_session_is_open_and_busy_only_when_a_tab_holds_it() {
        let summaries = [summary("a"), summary("b"), summary("c")];
        let open = [("a".to_owned(), true), ("b".to_owned(), false)];
        let published = with_tabs(&summaries, &open);
        assert_eq!(published.len(), 3);
        assert!(published[0].open && published[0].busy);
        assert!(published[1].open && !published[1].busy);
        assert!(!published[2].open && !published[2].busy);
        assert_eq!(published[2].title, "Chat c");
        assert_eq!(published[0].modified_at, "2026-10-07T09:00:00Z");
    }

    #[test]
    fn without_tabs_every_session_reads_as_closed() {
        let published = with_tabs(&[summary("a")], &[]);
        assert_eq!(published.len(), 1);
        assert!(!published[0].open);
        assert!(with_tabs(&[], &[("a".to_owned(), true)]).is_empty());
    }

    #[gpui_kit::test]
    fn publishing_identical_snapshots_does_not_wake_global_observers(cx: &mut TestAppContext) {
        let changes = Rc::new(Cell::new(0));
        let recorded = changes.clone();
        let _subscription = cx.update(|cx| {
            cx.observe_global::<ChatSessions>(move |_| recorded.set(recorded.get() + 1))
        });
        let snapshot = with_tabs(&[summary("a")], &[("a".into(), true)]);
        cx.update(|cx| publish(cx, snapshot.clone()));
        assert_eq!(changes.get(), 1);
        cx.update(|cx| {
            assert_eq!(sessions(cx), snapshot);
            publish(cx, snapshot.clone());
        });
        assert_eq!(changes.get(), 1, "identical publication must be silent");
        cx.update(|cx| publish(cx, with_tabs(&[summary("a")], &[])));
        assert_eq!(changes.get(), 2, "closing the tab must notify readers");
    }
}
