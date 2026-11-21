//! What the Copilot Agents dashboard's row actions do: opening a task's pages,
//! copying its link, naming its subject, showing a chat session and turning
//! monitoring on or off.

use gpui_kit::{Context, Window};
use subspace_framework::dashboard::{ChatRequest, CloudSession, DashboardEvent, Toast, links};

use crate::CopilotAgents;
use crate::keys::{
    CopyLink, OpenInChat, OpenPullRequest, OpenSession, OpenSubject, RetrySessions, ShowLog,
    ToggleMonitor,
};
use crate::model::{AgentRow, Kind, subject_key};
use crate::ui::monitor_message;

impl CopilotAgents {
    pub(crate) fn retry_sessions(
        &mut self,
        _: &RetrySessions,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.target().map(|row| row.key.clone()) {
            self.load_detail(&key, cx);
        }
    }

    pub(crate) fn open_session(&mut self, _: &OpenSession, _: &mut Window, cx: &mut Context<Self>) {
        let url = self
            .target()
            .and_then(crate::browser::link)
            .map(|link| link.url);
        match url.filter(|url| links::open(url, cx)) {
            Some(_) => {}
            None => say("This row has no session page on GitHub", cx),
        }
    }

    pub(crate) fn open_pull_request(
        &mut self,
        _: &OpenPullRequest,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url = self
            .target()
            .and_then(|row| row.pull_request.as_ref())
            .map(|pull| pull.url.clone());
        match url.filter(|url| links::open(url, cx)) {
            Some(_) => {}
            None => say("This task has no pull request", cx),
        }
    }

    pub(crate) fn copy_link(&mut self, _: &CopyLink, _: &mut Window, cx: &mut Context<Self>) {
        let url = self
            .target()
            .and_then(crate::browser::link)
            .map(|link| link.url);
        match url {
            Some(url) => {
                links::copy(url, cx);
                say("Copied the browser link", cx);
            }
            None => say("This row has no link to copy", cx),
        }
    }

    pub(crate) fn open_subject(&mut self, _: &OpenSubject, _: &mut Window, cx: &mut Context<Self>) {
        // Moving to another dashboard's item has no generic hook, so the
        // subject is named rather than navigated to.
        let subject = self
            .target()
            .and_then(|row| row.subject.as_ref())
            .map(subject_key);
        match subject {
            Some(key) => say(format!("Started from {key}"), cx),
            None => say("This task has no chat behind it", cx),
        }
    }

    pub(crate) fn open_in_chat(&mut self, _: &OpenInChat, _: &mut Window, cx: &mut Context<Self>) {
        match self.target().map(chat_request) {
            Some(request) => cx.emit(DashboardEvent::Chat(request)),
            None => say("Select a chat session or a cloud task first", cx),
        }
    }

    /// The log shows in the task's opened tab, so the task opens first.
    pub(crate) fn show_log(&mut self, _: &ShowLog, _: &mut Window, cx: &mut Context<Self>) {
        let key = self
            .target()
            .filter(|row| row.kind == Kind::Cloud)
            .map(|row| row.key.clone());
        match key {
            Some(key) => {
                self.open_item(&key, cx);
                self.toggle_log(cx);
            }
            None => say("Select a cloud task first", cx),
        }
    }

    /// Turns alerts for the selected task on or off. A task the file does
    /// not know gets a reference of its own, so the flag lasts.
    pub(crate) fn toggle_monitor(
        &mut self,
        _: &ToggleMonitor,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.target().filter(|row| row.kind == Kind::Cloud).cloned() else {
            say("Select a cloud task first", cx);
            return;
        };
        let Some(store) = self.store.clone() else {
            say(
                "Subspace has no state directory to record the choice in",
                cx,
            );
            return;
        };
        let wanted = !row.monitor;
        match crate::load::record_monitor(&store, &row, wanted) {
            Ok(()) => {
                self.saved = store.all().unwrap_or_default();
                self.rebuild(cx);
                say(monitor_message(&row.title, wanted), cx);
            }
            Err(error) => say(format!("Cannot record the choice: {error}"), cx),
        }
        cx.notify();
    }
}

/// One line of feedback from an action. A free function: it says something
/// through the shell and reads nothing of the dashboard.
pub(crate) fn say(text: impl Into<String>, cx: &mut Context<CopilotAgents>) {
    cx.emit(DashboardEvent::Toast(Toast::info(text.into())));
}

/// What Open in chat asks of the chat pane: a local session opens its saved
/// chat, a cloud task replays its last session read-only.
pub(crate) fn chat_request(row: &AgentRow) -> ChatRequest {
    match row.kind {
        Kind::Local => ChatRequest::OpenSession(row.id.clone().into()),
        Kind::Cloud => ChatRequest::OpenCloudSession(CloudSession {
            task_id: row.id.clone().into(),
            title: row.title.clone().into(),
            url: crate::browser::link(row)
                .map(|link| link.url)
                .unwrap_or_default(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::chat_request;
    use crate::model::{AgentRow, Kind};
    use subspace_framework::dashboard::ChatRequest;

    fn row(kind: Kind) -> AgentRow {
        AgentRow {
            key: "cloud:3f0a".into(),
            kind,
            id: "3f0a".into(),
            title: "Fix the flaky test".into(),
            repo: "shell/subspace".into(),
            state: "completed".into(),
            created_at: String::new(),
            updated_at: String::new(),
            monitor: false,
            subject: None,
            html_url: "https://github.com/copilot/agents/3f0a".into(),
            pull_request: None,
        }
    }

    #[test]
    fn a_local_row_opens_its_saved_chat_and_a_cloud_row_replays_its_session() {
        assert_eq!(
            chat_request(&row(Kind::Local)),
            ChatRequest::OpenSession("3f0a".into())
        );
        let ChatRequest::OpenCloudSession(task) = chat_request(&row(Kind::Cloud)) else {
            panic!("a cloud row asks for a replay");
        };
        assert_eq!(task.task_id, "3f0a");
        assert_eq!(task.title, "Fix the flaky test");
        assert_eq!(task.url, "https://github.com/copilot/agents/3f0a");
    }

    #[test]
    fn a_cloud_replay_without_rest_html_url_keeps_a_browser_destination() {
        let mut cloud = row(Kind::Cloud);
        cloud.html_url.clear();
        let ChatRequest::OpenCloudSession(task) = chat_request(&cloud) else {
            panic!("a cloud row asks for a replay");
        };
        assert_eq!(task.url, "https://github.com/copilot/agents");
    }
}
