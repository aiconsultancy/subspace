//! What an agent process reports: its lines, and the answers to its
//! permission requests that Subspace collected.
use super::chat::PermissionAsk;
use super::{AcpActor, ChatEvent, ChatId, Event, Incoming, Value, next_id};

impl AcpActor {
    pub(super) async fn receive(&mut self, process_id: u64, incoming: Incoming) {
        let Some(id) = self
            .chats
            .iter()
            .find_map(|(id, chat)| (chat.process_id == process_id).then_some(*id))
        else {
            return;
        };
        let result = match incoming {
            Incoming::Record(record) => self.receive_record(id, &record).await,
            Incoming::Permission { ask, allowed } => {
                self.answer_permission(id, &ask, allowed).await
            }
            Incoming::Exited(message) => {
                self.process_exit(id, &message).await;
                return;
            }
        };
        let Err(error) = result else {
            self.update_library(id);
            return;
        };
        if self.chats.get(&id).is_some_and(|chat| !chat.ready) {
            self.process_exit(id, &error).await;
        } else {
            self.events
                .push(Event::of(id, ChatEvent::Error(error)))
                .await;
        }
    }

    async fn receive_record(&mut self, id: ChatId, record: &Value) -> Result<(), String> {
        let Some(chat) = self.chats.get_mut(&id) else {
            return Ok(());
        };
        if let Some(ask) = chat.receive(record, &self.events).await? {
            let process_id = chat.process_id;
            self.ask_permission(id, process_id, ask);
        }
        let queued = self
            .chats
            .get_mut(&id)
            .filter(|chat| chat.ready)
            .and_then(|chat| chat.queued.take());
        if self.chats.get(&id).is_some_and(|chat| chat.ready) {
            self.recoveries.remove(&id);
        }
        if let Some(command) = queued {
            self.dispatch(
                super::Command::Chat { chat: id, command },
                &Default::default(),
            )
            .await;
        }
        Ok(())
    }

    /// Shows the request on the approval card without blocking the agent's
    /// other messages; the answer comes back as [`Incoming::Permission`].
    fn ask_permission(&self, chat: ChatId, process_id: u64, ask: PermissionAsk) {
        let decisions = self.decisions.clone();
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let request = format!("acp-{}", next_id());
            let allowed = decisions
                .approval(chat, request, ask.summary.clone())
                .await
                .unwrap_or(false);
            let _ = sender
                .send((process_id, Incoming::Permission { ask, allowed }))
                .await;
        });
    }

    async fn answer_permission(
        &mut self,
        id: ChatId,
        ask: &PermissionAsk,
        allowed: bool,
    ) -> Result<(), String> {
        match self.chats.get_mut(&id) {
            Some(chat) => chat.answer_permission(ask, allowed).await,
            None => Ok(()),
        }
    }
}
