use super::super::chat::Chat;
use super::super::setup::chat_session;
use super::super::startup::room_for_a_tab;
use super::super::tabs::SavedTabs;
use super::super::{ChatId, ChatPreferences, Event, MAX_CHATS, SessionId};
use super::{Actor, parse_session_id};

/// The chat id of the `index`th restored tab. The panel continues numbering
/// past this range, so a restored id never collides with one of its own.
pub(super) fn chat_id(index: usize) -> ChatId {
    ChatId::try_from(index + 1).unwrap_or(1)
}

/// The chat the panel shows after a restore: the saved active tab, else the
/// first one that reopened.
pub(super) fn restored_active(restored: &[(ChatId, String)], active_index: usize) -> ChatId {
    restored
        .get(active_index)
        .or_else(|| restored.first())
        .map_or(1, |(chat, _)| *chat)
}

impl Actor {
    /// Reopens the saved tabs, keeping the ones that still resume. A chat
    /// that cannot be resumed is reported once and dropped from the list.
    pub(super) async fn restore(&mut self, saved: SavedTabs) -> Result<(), String> {
        let ids = saved.session_ids();
        let active_index = saved.active_index();
        let mut restored: Vec<(ChatId, String)> = Vec::new();
        for (index, id) in ids.iter().take(MAX_CHATS).enumerate() {
            match self.open_chat(chat_id(index), Some(id.clone()), None).await {
                Ok(()) => restored.push((chat_id(index), id.as_str().to_owned())),
                Err(error) => self.events.push(Event::Error(error)).await,
            }
        }
        if restored.is_empty() {
            let chat = chat_id(ids.len());
            self.open_chat(chat, None, Some(self.preferences.clone()))
                .await?;
            restored.push((chat, chat_session(self.chats.get(&chat))));
        }
        self.active = restored_active(&restored, active_index);
        let active = self.active;
        // The panel merges this into the tabs these chats already filled.
        self.events
            .push(Event::Restore {
                tabs: restored,
                active,
            })
            .await;
        self.save_tabs()
    }

    /// Opens a chat Zack asked for, within the tab limit.
    pub(super) async fn open_requested(
        &mut self,
        id: ChatId,
        session: Option<String>,
        preferences: Option<ChatPreferences>,
    ) -> Result<(), String> {
        room_for_a_tab(self.chats.len())?;
        let saved = session.as_deref().map(parse_session_id).transpose()?;
        self.open_chat(
            id,
            saved,
            preferences.or_else(|| (!session.is_some()).then(|| self.preferences.clone())),
        )
        .await?;
        self.active = id;
        self.save_tabs()
    }

    pub(super) async fn open_chat(
        &mut self,
        id: ChatId,
        saved: Option<SessionId>,
        preferences: Option<ChatPreferences>,
    ) -> Result<(), String> {
        let chat = Chat::open(&self.env(), id, saved, self.sessions.clone(), preferences).await?;
        self.insert_chat(id, chat);
        Ok(())
    }
}
