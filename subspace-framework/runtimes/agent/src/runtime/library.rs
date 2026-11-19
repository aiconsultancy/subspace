//! The commands that act on this account's saved chats rather than on one
//! open tab: naming, deleting and exporting them, and recording which tab
//! Zack is in.

use super::actor::Actor;
use super::export::{markdown, session_history, write_export};
use super::names::{display_name, first_message, validate_name};
use super::session::{rename_saved_session, saved_session_name};
use super::{ChatCommand, ChatId, Event, PathBuf, SessionId, parse_session_id, publish_sessions};

impl Actor {
    /// The tab Zack is now in. A restart reopens it as the active one.
    pub(super) fn activate(&mut self, chat: ChatId) -> Result<(), String> {
        if !self.chats.contains_key(&chat) {
            return Ok(());
        }
        self.active = chat;
        self.save_tabs()
    }

    /// The chat holding `session`, if one is open.
    fn chat_of(&self, session: &SessionId) -> Option<ChatId> {
        self.chats
            .iter()
            .find(|(_, chat)| chat.session.id().as_str() == session.as_str())
            .map(|(id, _)| *id)
    }

    /// Names a saved chat. An open chat is renamed through its own session,
    /// so its tab title follows; a closed one is renamed without resuming it.
    pub(super) async fn rename_session(
        &mut self,
        session: String,
        name: String,
    ) -> Result<(), String> {
        let id = parse_session_id(&session)?;
        let name = validate_name(&name)?;
        match self.chat_of(&id) {
            Some(chat) => {
                self.chat_command(chat, ChatCommand::Rename(name)).await?;
            }
            None => rename_saved_session(&self.client, &id, &name).await?,
        }
        publish_sessions(&self.client, &self.events).await;
        Ok(())
    }

    /// Detaches the tab holding `session`, when one does.
    async fn close_session_tab(&mut self, session: &SessionId) -> Result<(), String> {
        match self.chat_of(session) {
            Some(chat) => self.close_chat(chat).await,
            None => Ok(()),
        }
    }

    /// Deletes a saved chat. Its tab is detached first, so the SDK is not
    /// holding the session when it is removed.
    pub(super) async fn delete_session(&mut self, session: String) -> Result<(), String> {
        let id = parse_session_id(&session)?;
        self.close_session_tab(&id).await?;
        self.client
            .delete_session(&id)
            .await
            .map_err(|_| "Copilot could not delete that chat.".to_string())?;
        publish_sessions(&self.client, &self.events).await;
        Ok(())
    }

    /// Writes a saved chat's transcript as Markdown, without resuming it.
    pub(super) async fn export_session(
        &mut self,
        session: String,
        path: PathBuf,
    ) -> Result<(), String> {
        let id = parse_session_id(&session)?;
        let entries = session_history(&self.client, id.clone()).await?;
        let name = saved_session_name(&self.client, &id).await;
        let title = display_name(name.as_deref(), first_message(&entries));
        write_export(&path, &markdown(&title, &entries))?;
        self.events
            .push(Event::Notice(format!(
                "Exported the chat to {}.",
                path.display()
            )))
            .await;
        Ok(())
    }
}
