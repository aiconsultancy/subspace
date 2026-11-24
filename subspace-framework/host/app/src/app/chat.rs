//! The chat pane: opening it and pinning a row or a selection to it.

use std::rc::Rc;

use gpui_kit::{App, Context, Div, InteractiveElement as _, Stateful, Window};
use subspace_chat::{ChatAttentionKind, OPEN_KEY as CHAT_OPEN_KEY};
use subspace_dashboard::{
    AddSelectionToChat, AddToChat, AgentContext, ChatRequest, CloudSession, Toast, context_text,
    prefs,
};

use super::{SubspaceApp, ToggleChat};
use crate::notifications::{NotificationPreferences, alert_text, play_sound, show_system_alert};

/// The toast when nothing is selected to add to the chat.
const NOTHING_TO_ADD: &str = "Select a row first: there is nothing to add to the chat.";

impl SubspaceApp {
    pub(super) fn chat_attention(
        &mut self,
        kind: ChatAttentionKind,
        chat: u64,
        cx: &mut Context<Self>,
    ) {
        let preferences = NotificationPreferences::load(cx);
        if !preferences.allows(kind) {
            return;
        }
        self.push_toast(Toast::info(alert_text(kind)), cx);
        if preferences.os {
            show_system_alert(kind, chat, cx);
        }
        if preferences.sound {
            play_sound(cx);
        }
    }

    /// What a dashboard asked of the chat pane. The pane owns the runtime,
    /// so every dashboard reaches it through this one generic hook.
    pub(super) fn chat_request(
        &mut self,
        request: &ChatRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            ChatRequest::ListSessions => self.chat.update(cx, |chat, cx| chat.refresh_sessions(cx)),
            ChatRequest::OpenSession(session) => self.open_chat_session(session, window, cx),
            ChatRequest::OpenCloudSession(task) => self.open_cloud_session(task, window, cx),
            ChatRequest::OpenInSubspace(citation) => self.open_in_subspace(citation, window, cx),
            ChatRequest::RefreshVisibleBoard => self.refresh_visible_board(window, cx),
        }
    }

    pub(super) fn open_in_subspace(
        &mut self,
        citation: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = subspace_dashboard::full_text::parse_link(citation) else {
            self.push_toast(
                Toast::info("This citation is not a valid Subspace result."),
                cx,
            );
            return;
        };
        self.open_indexed_target(target, window, cx);
    }

    pub(super) fn refresh_visible_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.main.is_board() {
            return;
        }
        if let Some(board) = self.dashboard()
            && !board.busy(cx)
        {
            board.reload(window, cx);
        }
    }

    /// Opens the pane when it is closed, then replays the cloud agent task.
    fn open_cloud_session(
        &mut self,
        task: &CloudSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.chat_open {
            self.toggle_chat(&ToggleChat, window, cx);
        }
        self.chat
            .update(cx, |chat, cx| chat.show_cloud_session(task, window, cx));
    }

    /// Opens the pane when it is closed, then shows the saved session.
    fn open_chat_session(&mut self, session: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chat_open {
            self.toggle_chat(&ToggleChat, window, cx);
        }
        self.chat
            .update(cx, |chat, cx| chat.show_session(session, window, cx));
    }

    pub(super) fn install_chat_context(&self, cx: &mut Context<Self>) {
        let app = cx.weak_entity();
        self.chat.update(cx, |chat, _| {
            chat.set_context_provider(Rc::new(move |cx| {
                app.read_with(cx, |app, cx| app.visible_board_context(cx))
                    .ok()
                    .flatten()
            }));
            chat.set_chat_tools(Rc::new(crate::mcp::chat_tools));
        });
    }

    fn visible_board_context(&self, cx: &App) -> Option<AgentContext> {
        self.dashboard()
            .filter(|_| self.main.is_board())
            .and_then(|board| board.agent_context(cx))
    }

    pub(super) fn toggle_chat(
        &mut self,
        _: &ToggleChat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if super::sign_in_gate::shown(cx) {
            return;
        }
        let open = self.change_chat_visibility();
        prefs::set(cx, CHAT_OPEN_KEY, chat_open_preference(open));
        let _ = (!open).then(|| {
            self.chat
                .update(cx, |chat, cx| chat.dismiss_popups(window, cx))
        });
        let _ = open.then(|| self.chat.update(cx, |chat, cx| chat.on_open(window, cx)));
        let _ = (!open).then(|| self.focus_active(window, cx));
        cx.notify();
    }

    pub(super) fn change_chat_visibility(&mut self) -> bool {
        self.chat_open = !self.chat_open;
        self.chat_open
    }

    pub(super) fn add_to_chat(
        &mut self,
        _: &AddToChat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let reference = self.visible_board_context(cx);
        self.attach_reference(reference, window, cx);
    }

    pub(super) fn add_selection_to_chat(
        &mut self,
        _: &AddSelectionToChat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = self.selection_source(cx);
        let selected = subspace_dashboard::take_selection(cx);
        let reference = context_text::selection_reference(&source, &selected);
        self.attach_reference(reference, window, cx);
    }

    pub(super) fn selection_source(&self, cx: &App) -> String {
        let dashboard = self.dashboard();
        let label = dashboard.and_then(|board| board.agent_context(cx));
        label.map(|context| context.label).unwrap_or_else(|| {
            dashboard
                .map(|board| board.title.to_string())
                .unwrap_or_default()
        })
    }

    pub(super) fn attach_reference(
        &mut self,
        reference: Option<AgentContext>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reference) = reference else {
            self.push_toast(Toast::info(NOTHING_TO_ADD), cx);
            return;
        };
        if !self.chat_open {
            self.toggle_chat(&ToggleChat, window, cx);
        }
        if self.chat.read(cx).is_disabled() {
            return;
        }
        let label = reference.label.clone();
        self.chat
            .update(cx, |chat, cx| chat.attach_reference(reference, cx));
        self.push_toast(Toast::info(added_message(&label)), cx);
    }

    pub(super) fn chat_actions(shell: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        shell
            .on_action(cx.listener(Self::toggle_chat))
            .on_action(cx.listener(Self::add_to_chat))
            .on_action(cx.listener(Self::add_selection_to_chat))
    }
}

fn chat_open_preference(open: bool) -> &'static str {
    if open { "1" } else { "0" }
}

fn added_message(label: &str) -> String {
    format!("Added to the chat: {label}")
}

#[cfg(test)]
mod tests {
    use super::{added_message, chat_open_preference};

    #[test]
    pub(super) fn the_chat_choice_is_stored_as_one_or_zero() {
        assert_eq!(chat_open_preference(true), "1");
        assert_eq!(chat_open_preference(false), "0");
    }

    #[test]
    pub(super) fn a_pinned_reference_names_its_row() {
        assert_eq!(added_message("PR 12"), "Added to the chat: PR 12");
    }
}
