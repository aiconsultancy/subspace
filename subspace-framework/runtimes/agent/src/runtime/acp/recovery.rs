//! A conversation survives failed process replacement until setup succeeds.
use super::chat::AcpChat;
use super::protocol::ModelOptions;
use super::transcript::AcpTranscript;
use super::{AcpActor, ChatEvent, ChatId, Event, TranscriptEntry, json};

#[derive(Clone)]
pub(super) struct Snapshot {
    saved: Option<String>,
    title: String,
    created_at: String,
    entries: Vec<TranscriptEntry>,
    transcript: AcpTranscript,
    models: ModelOptions,
    position: usize,
}

impl Snapshot {
    fn capture(chat: &mut AcpChat, position: usize) -> Self {
        let mut transcript = std::mem::take(&mut chat.transcript);
        transcript.restart();
        Self {
            saved: chat
                .session_id
                .clone()
                .filter(|_| chat.capabilities.load_session),
            title: std::mem::take(&mut chat.title),
            created_at: std::mem::take(&mut chat.created_at),
            entries: std::mem::take(&mut chat.entries),
            transcript,
            models: std::mem::take(&mut chat.models),
            position,
        }
    }

    fn restore(&self, chat: &mut AcpChat) {
        chat.recovering = true;
        chat.title.clone_from(&self.title);
        chat.created_at.clone_from(&self.created_at);
        chat.entries.clone_from(&self.entries);
        chat.transcript = self.transcript.clone();
        chat.preferred_model.clone_from(&self.models.current);
        chat.preferred_effort.clone_from(&self.models.effort);
    }
}

impl AcpActor {
    pub(super) async fn recover_cancelled(&mut self, id: ChatId) -> Result<(), String> {
        self.decisions.cancel(id);
        let Some(chat) = self.chats.get_mut(&id) else {
            return Ok(());
        };
        chat.turn_ended(Ok(&json!({"stopReason":"cancelled"})), &self.events)
            .await?;
        let position = self
            .order
            .iter()
            .position(|open| *open == id)
            .unwrap_or(self.order.len());
        self.recoveries
            .insert(id, Snapshot::capture(chat, position));
        self.retire_replacement(id).await;
        self.start_replacement(id).await
    }

    pub(super) async fn start_replacement(&mut self, id: ChatId) -> Result<(), String> {
        let Some(snapshot) = self.recoveries.get(&id).cloned() else {
            return Ok(());
        };
        self.open_chat(id, snapshot.saved.clone(), None)
            .await
            .map_err(|error| self.restart_problem(&error))?;
        self.order.retain(|open| *open != id);
        self.order
            .insert(snapshot.position.min(self.order.len()), id);
        if let Some(chat) = self.chats.get_mut(&id) {
            snapshot.restore(chat);
        }
        Ok(())
    }

    pub(super) fn restart_problem(&self, error: &str) -> String {
        format!(
            "{} couldn't restart. Send another message to try again. {error}",
            self.agent
        )
    }

    pub(super) async fn replacement_failed(&mut self, id: ChatId, error: &str) {
        let queued = self
            .chats
            .get(&id)
            .is_some_and(|chat| chat.queued.is_some());
        self.retire_replacement(id).await;
        self.events
            .push(Event::of(id, ChatEvent::Error(self.restart_problem(error))))
            .await;
        if queued {
            self.events.push(Event::of(id, ChatEvent::TurnDone)).await;
        }
    }

    async fn retire_replacement(&mut self, id: ChatId) {
        self.order.retain(|open| *open != id);
        if let Some(mut chat) = self.chats.remove(&id) {
            let _ = chat.child.start_kill();
            chat.stop().await;
        }
    }
}
