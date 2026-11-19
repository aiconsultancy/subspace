//! What the Sessions tab does: opening, renaming, exporting and deleting a
//! saved chat, and the keys that drive the list.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{Entity, Window};
use subspace_agent::{Command, SessionSummary, export_file_name};

use super::super::{ChatPanel, Context};
use super::view::RowAction;
use super::{listed, moved_selection};

impl ChatPanel {
    /// Shows the Sessions tab and asks the runtime for a fresh list.
    pub(in crate::agent_chat) fn show_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_popups(window, cx);
        self.sessions_open = true;
        self.view.notice = None;
        self.send(Command::ListSessions, cx);
        cx.notify();
    }

    /// One entry of a row's context menu.
    pub(in crate::agent_chat) fn row_action(
        &mut self,
        action: RowAction,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session_by_id(id) else {
            return;
        };
        match action {
            RowAction::Rename => self.start_session_rename(&session, window, cx),
            RowAction::Export => self.export_session(&session, cx),
            RowAction::Delete => {
                self.view.confirm_delete = Some(session);
                cx.notify();
            }
        }
    }

    pub(in crate::agent_chat) fn session_by_id(&self, id: &str) -> Option<SessionSummary> {
        self.sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
    }

    /// Opens the editor over the row's name, with the current name selected.
    pub(in crate::agent_chat) fn start_session_rename(
        &mut self,
        session: &SessionSummary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = session.title.to_string();
        self.view.renaming = Some(session.id.clone());
        self.view.rename.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    pub(in crate::agent_chat) fn cancel_session_rename(&mut self, cx: &mut Context<Self>) {
        self.view.renaming = None;
        cx.notify();
    }

    /// Saves the edited name. The runtime confirms it by republishing the
    /// list, so the row is not renamed locally first.
    pub(in crate::agent_chat) fn save_session_rename(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.view.renaming.take() else {
            return;
        };
        let name = self.view.rename.read(cx).value().to_string();
        self.send(Command::RenameSession { session, name }, cx);
        // The list stays open: renaming a chat is not opening it.
        cx.notify();
    }

    /// Asks for a file, then has the runtime write the transcript there.
    pub(in crate::agent_chat) fn export_session(
        &mut self,
        session: &SessionSummary,
        cx: &mut Context<Self>,
    ) {
        self.view.notice = None;
        let id = session.id.clone();
        let name = export_file_name(&session.title);
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let path = cx.prompt_for_new_path(&home, Some(&name));
        cx.spawn(async move |chat, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return;
            };
            let _ = chat.update(cx, |chat, cx| {
                chat.send(
                    Command::ExportSession {
                        session: id.clone(),
                        path,
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    /// Deletes the chat the dialog named, closing its tab first.
    pub(in crate::agent_chat) fn delete_confirmed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.view.confirm_delete.take() else {
            return;
        };
        if let Some(index) = super::super::tabs::session_tab(&self.tabs, &session.id)
            && let Some(id) = self.tabs.get(index).map(|tab| tab.id)
        {
            self.close_tab(id, window, cx);
        }
        self.send(Command::DeleteSession(session.id), cx);
        cx.notify();
    }

    /// Opens a row's chat. A row being renamed keeps its click, so saving a
    /// name never leaves the list.
    pub(in crate::agent_chat) fn open_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view.renaming.is_some() {
            return;
        }
        self.open_session(id, window, cx);
    }

    /// Moves the highlight, clamped to the listed rows.
    pub(in crate::agent_chat) fn move_highlight(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.view.renaming.is_some() {
            return;
        }
        let count = listed(&self.sessions, &self.view.query).len();
        self.view.selected = moved_selection(count, self.view.selected, step);
        cx.notify();
    }

    /// Enter opens the highlighted chat, unless a row is being renamed: the
    /// editor owns Enter then, and saving a name stays in the list.
    pub(in crate::agent_chat) fn open_highlighted(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.highlighted().filter(|_| self.view.renaming.is_none()) else {
            return;
        };
        self.open_session(&session.id, window, cx);
    }

    /// F2 renames the highlighted chat.
    pub(in crate::agent_chat) fn rename_highlighted(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.highlighted() else {
            return;
        };
        self.start_session_rename(&session, window, cx);
    }

    /// Delete asks before removing the highlighted chat. While a row is
    /// being renamed the key edits the name instead.
    pub(in crate::agent_chat) fn delete_highlighted(&mut self, cx: &mut Context<Self>) {
        if self.view.renaming.is_some() {
            return;
        }
        self.view.confirm_delete = self.highlighted();
        cx.notify();
    }

    pub(in crate::agent_chat) fn on_search_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.view.query = self.view.search.read(cx).value().to_string();
            self.view.selected = 0;
            cx.notify();
        }
    }

    pub(in crate::agent_chat) fn on_session_rename_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => self.save_session_rename(cx),
            InputEvent::Blur => self.cancel_session_rename(cx),
            _ => {}
        }
    }
}
