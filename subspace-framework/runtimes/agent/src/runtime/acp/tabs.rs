//! Opening and closing tabs: each tab starts its own agent process.
use super::chat::{AcpChat, SpawnConfig};
use super::mcp::SessionTools;
use super::sessions::validate_session_id;
use super::{AcpActor, ChatEvent, ChatId, ChatPreferences, Event, MAX_CHATS};
use crate::runtime::decisions::ChatApproval;
use std::sync::Arc;

impl AcpActor {
    pub(super) async fn open_requested(
        &mut self,
        id: ChatId,
        session: Option<String>,
        preferences: Option<ChatPreferences>,
    ) -> Result<(), String> {
        if self.chats.len() >= MAX_CHATS {
            return Err(format!(
                "At most {MAX_CHATS} chats are open at once. Close a tab first."
            ));
        }
        if let Some(saved) = session.as_deref() {
            validate_session_id(saved)?;
        }
        self.open_chat(id, session, preferences).await?;
        self.active = id;
        self.save_tabs()
    }

    pub(super) async fn open_chat(
        &mut self,
        id: ChatId,
        saved: Option<String>,
        preferences: Option<ChatPreferences>,
    ) -> Result<(), String> {
        let title = saved
            .as_ref()
            .and_then(|session| self.library.items.get(session))
            .map(|item| item.title.as_str());
        let config = SpawnConfig {
            launch: &self.launch,
            agent: self.agent,
            cwd: &self.files.workspace,
            tools: SessionTools::new(
                self.tools.clone(),
                Arc::new(ChatApproval {
                    decisions: self.decisions.clone(),
                    chat: id,
                }),
            ),
            saved: saved.as_deref(),
            title,
        };
        let mut chat = AcpChat::spawn(config, id, self.sender.clone()).await?;
        chat.preferred_model = preferences.and_then(|preferences| preferences.acp_model);
        self.chats.insert(id, chat);
        self.order.retain(|open| *open != id);
        self.order.push(id);
        Ok(())
    }

    pub(super) async fn close_chat(&mut self, id: ChatId) -> Result<(), String> {
        self.recoveries.remove(&id);
        if let Some(chat) = self.chats.remove(&id) {
            self.decisions.cancel(id);
            chat.stop().await;
        }
        self.order.retain(|open| *open != id);
        if self.active == id {
            self.active = self.order.last().copied().unwrap_or(id);
        }
        self.save_tabs()
    }

    pub(super) async fn process_exit(&mut self, id: ChatId, message: &str) {
        self.decisions.cancel(id);
        if self
            .chats
            .get(&id)
            .is_some_and(|chat| chat.cancel_deadline.is_some())
        {
            if let Err(error) = self.recover_cancelled(id).await {
                self.events
                    .push(Event::of(id, ChatEvent::Error(error)))
                    .await;
            }
            return;
        }
        if self.recoveries.contains_key(&id) {
            self.replacement_failed(id, message).await;
            return;
        }
        let Some(chat) = self.chats.remove(&id) else {
            return;
        };
        chat.stop().await;
        self.order.retain(|open| *open != id);
        self.events
            .push(Event::of(id, ChatEvent::Error(message.to_owned())))
            .await;
        self.events.push(Event::of(id, ChatEvent::TurnDone)).await;
        let _ = self.save_tabs();
    }
}
