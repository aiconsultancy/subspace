//! The saved chats of one agent: Subspace's library of their titles, and the
//! commands that name, delete and export them.
use super::{
    AcpActor, ChatId, Event, LibraryItem, Path, SESSION_ID_LIMIT, markdown_with_agent,
    save_library, validate_name, write_export,
};

impl AcpActor {
    pub(super) fn update_library(&mut self, id: ChatId) {
        let Some(chat) = self.chats.get(&id) else {
            return;
        };
        let Some(session) = chat.session_id.clone() else {
            return;
        };
        let item = LibraryItem {
            title: chat.title.clone(),
            path: String::new(),
            modified_at: chrono::Utc::now().to_rfc3339(),
            created_at: chat.created_at.clone(),
        };
        let changed = self
            .library
            .items
            .get(&session)
            .is_none_or(|known| known.title != item.title);
        self.library.items.insert(session, item);
        if changed {
            let _ = save_library(&self.library, &self.files.library);
        }
    }

    pub(super) async fn publish_library(&self) -> Result<(), String> {
        self.events
            .push(Event::Sessions(self.library.summaries()))
            .await;
        Ok(())
    }

    pub(super) async fn rename_session(&mut self, session: &str, name: &str) -> Result<(), String> {
        validate_session_id(session)?;
        let name = validate_name(name)?;
        let open = self
            .chats
            .values_mut()
            .find(|chat| chat.session_id.as_deref() == Some(session));
        if let Some(chat) = open {
            chat.rename(&name, &self.events).await?;
        }
        let item = self
            .library
            .items
            .get_mut(session)
            .ok_or_else(|| "That chat is not in Subspace's library.".to_string())?;
        item.title = name;
        save_library(&self.library, &self.files.library)?;
        self.publish_library().await
    }

    pub(super) async fn delete_session(&mut self, session: &str) -> Result<(), String> {
        validate_session_id(session)?;
        let open = self
            .chats
            .iter()
            .find_map(|(id, chat)| (chat.session_id.as_deref() == Some(session)).then_some(*id));
        if let Some(chat) = open {
            self.close_chat(chat).await?;
        }
        self.library
            .items
            .remove(session)
            .ok_or_else(|| "That chat is not in Subspace's library.".to_string())?;
        save_library(&self.library, &self.files.library)?;
        self.publish_library().await
    }

    /// Exports an open chat's transcript. A closed chat's history stays with
    /// the agent, which replays it only into an open session.
    pub(super) async fn export_session(&self, session: &str, path: &Path) -> Result<(), String> {
        validate_session_id(session)?;
        let chat = self
            .chats
            .values()
            .find(|chat| chat.session_id.as_deref() == Some(session))
            .ok_or_else(|| "Open the chat in a tab to export it.".to_string())?;
        write_export(
            path,
            &markdown_with_agent(&chat.title, &chat.entries, self.agent),
        )?;
        let notice = format!("Exported the chat to {}.", path.display());
        self.events.push(Event::Notice(notice)).await;
        Ok(())
    }
}

/// Agents choose their own session ids; Subspace accepts the printable,
/// path-free ones it can store and pass back.
pub(super) fn validate_session_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id.len() <= SESSION_ID_LIMIT
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'));
    valid
        .then_some(())
        .ok_or_else(|| "That ACP session id is invalid.".into())
}
