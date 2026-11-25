//! Opening, choosing and closing chat tabs, and the saved chats they show.

use super::interaction::chat_preferences;
use super::tabs::{
    SessionChoice, Tab, can_open, choose_session, index_after_close, stepped_index, tab_index,
};
use super::{ChatCommand, ChatId, ChatPanel, Command, Context, Window};
use crate::CloudSession;

impl ChatPanel {
    /// Shows the saved session `id` in a chat tab: its own tab when one
    /// holds it, else a new tab. The shell opens the pane first.
    pub fn show_session(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_session(id, window, cx);
    }

    /// The user chose a saved chat: its tab if it is open, else a new tab.
    /// Either way the panel leaves the Sessions tab.
    pub(super) fn focus_chat(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.sessions_open = false;
        self.select_tab(index, window, cx);
    }

    pub(super) fn select_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || (index == self.active && !self.sessions_open) {
            return;
        }
        self.remember_draft(cx);
        self.renaming = None;
        self.sessions_open = false;
        self.active = index;
        self.show_draft(window, cx);
        // The runtime saves the tab the user is in, so a restart reopens it.
        let chat = self.tab().id;
        self.send(Command::Activate(chat), cx);
        self.sync_toolbar(cx);
        cx.notify();
    }

    pub(super) fn step_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let index = stepped_index(self.tabs.len(), self.active, step);
        self.select_tab(index, window, cx);
    }

    /// Opens an empty chat in a new tab.
    pub(super) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_tab(None, window, cx);
    }

    fn open_tab(&mut self, session: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat) = self.claim_chat(cx) else {
            return;
        };
        let opened = if session.is_none() {
            self.send(new_chat_command(chat, cx), cx)
        } else {
            self.send(
                Command::OpenChat {
                    chat,
                    session,
                    preferences: None,
                },
                cx,
            )
        };
        let _ = opened.then(|| self.place_tab(Tab::opened(chat), window, cx));
    }

    /// Replays a cloud agent task's last session in a read-only tab. Every
    /// replay opens a fresh tab: the connected session is transient, so
    /// there is no saved chat to focus instead. The shell opens the pane
    /// first.
    pub fn show_cloud_session(
        &mut self,
        task: &CloudSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_disabled() {
            return;
        }
        if !self.capabilities().cloud_replay() {
            self.tab_mut()
                .set_error(replay_unavailable(subspace_agent::COPILOT_BUILT_IN).into());
            self.log_accepted_errors(cx);
            cx.notify();
            return;
        }
        let Some(chat) = self.claim_chat(cx) else {
            return;
        };
        let command = Command::OpenRemote {
            chat,
            task_id: task.task_id.to_string(),
        };
        let opened = self.send(command, cx);
        let _ = opened.then(|| self.place_tab(Tab::replay(chat, task), window, cx));
    }

    /// The next chat id, once there is room for another tab. Refusing says
    /// so in the tab the user is in and hands out no id.
    fn claim_chat(&mut self, cx: &mut Context<Self>) -> Option<ChatId> {
        if let Err(refused) = can_open(self.tabs.len()) {
            self.tab_mut().set_error(refused);
            self.log_accepted_errors(cx);
            cx.notify();
            return None;
        }
        self.remember_draft(cx);
        let chat = self.next_chat;
        self.next_chat += 1;
        Some(chat)
    }

    /// Puts an opened tab at the end of the strip and shows it.
    fn place_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.renaming = None;
        self.sessions_open = false;
        self.show_draft(window, cx);
        self.sync_toolbar(cx);
        cx.notify();
    }

    /// Detaches a chat's session and drops its tab. The session stays saved.
    pub(super) fn close_tab(&mut self, id: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = tab_index(&self.tabs, id) else {
            return;
        };
        // The panel always has a tab: the last one is replaced before it goes,
        // and stays if the replacement cannot open.
        if self.tabs.len() == 1 {
            self.new_tab(window, cx);
        }
        if self.tabs.len() == 1 {
            return;
        }
        self.send(
            Command::Chat {
                chat: id,
                command: ChatCommand::Close,
            },
            cx,
        );
        self.remember_draft(cx);
        self.active = index_after_close(self.tabs.len(), index, self.active);
        self.tabs.remove(index);
        self.renaming = None;
        self.show_draft(window, cx);
        self.sync_toolbar(cx);
        cx.notify();
    }

    /// A chosen saved chat: its tab if it is open, else a new tab.
    pub(super) fn open_session(
        &mut self,
        session: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choose_session(&self.tabs, session) {
            SessionChoice::Focus(index) => self.focus_chat(index, window, cx),
            SessionChoice::Open(session) => self.open_tab(Some(session), window, cx),
        }
    }
}

/// Why a cloud session does not open: only Copilot replays one, and a build
/// without Copilot cannot switch to it.
fn replay_unavailable(copilot_built_in: bool) -> &'static str {
    if copilot_built_in {
        "Cloud-session replay is unavailable with Pi. Choose Copilot in Settings and restart Subspace to open this session."
    } else {
        "Cloud-session replay needs Copilot chat, which this build of Subspace does not include. Open the task on GitHub instead."
    }
}

fn new_chat_command(chat: ChatId, cx: &super::App) -> Command {
    Command::OpenChat {
        chat,
        session: None,
        preferences: Some(chat_preferences(cx)),
    }
}

#[cfg(test)]
mod tests {
    use super::{new_chat_command, replay_unavailable};
    use crate::prefs;
    use subspace_agent::Command;

    #[test]
    fn a_build_without_copilot_does_not_suggest_choosing_it_for_a_replay() {
        assert!(replay_unavailable(true).contains("Choose Copilot in Settings"));
        let missing = replay_unavailable(false);
        assert!(missing.contains("does not include"), "{missing}");
        assert!(!missing.contains("Choose Copilot"), "{missing}");
    }

    #[gpui_kit::test]
    fn same_app_context_refreshes_new_chat_snapshot_without_mutating_old_one(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            prefs::set(cx, "chat.default-model.copilot", "gpt-5");
            let first = new_chat_command(4, cx);
            prefs::set(cx, "chat.approve-all-default", "1");
            let second = new_chat_command(5, cx);
            prefs::set(cx, "unrelated.preference", "kept");
            prefs::save(cx).unwrap();
            assert!(matches!(
                first,
                Command::OpenChat {
                    preferences: Some(found), ..
                } if found.copilot_model.as_deref() == Some("gpt-5") && !found.approve_all
            ));
            assert!(matches!(
                second,
                Command::OpenChat {
                    preferences: Some(found), ..
                } if found.copilot_model.as_deref() == Some("gpt-5") && found.approve_all
            ));
            assert_eq!(
                prefs::get(cx, "chat.default-model.copilot").as_deref(),
                Some("gpt-5")
            );
            assert_eq!(
                prefs::get(cx, "unrelated.preference").as_deref(),
                Some("kept")
            );
            let restored = Command::OpenChat {
                chat: 6,
                session: Some("saved".into()),
                preferences: None,
            };
            assert!(matches!(
                restored,
                Command::OpenChat {
                    preferences: None,
                    ..
                }
            ));
        });
    }
}
