//! Bounded Stop and process replacement, isolated from stale agent output.
use super::{AcpActor, ChatEvent, ChatId, Event};
use std::time::Duration;
use tokio::time::Instant;

/// How long Stop lets the agent finish before ending the turn locally.
pub(super) const CANCEL_GRACE_PERIOD: Duration = Duration::from_secs(3);

pub(super) async fn wait(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

impl AcpActor {
    pub(super) async fn cancel_chat(&mut self, id: ChatId) -> Result<(), String> {
        self.decisions.cancel(id);
        if !self.chats.contains_key(&id) && self.recoveries.contains_key(&id) {
            self.events.push(Event::of(id, ChatEvent::TurnDone)).await;
            return Ok(());
        }
        let chat = self
            .chats
            .get_mut(&id)
            .ok_or_else(|| "That chat is no longer open.".to_owned())?;
        if chat.queued.take().is_some() || !chat.busy {
            self.events.push(Event::of(id, ChatEvent::TurnDone)).await;
        }
        if chat.cancel().await.is_err() {
            self.recover_cancelled(id).await?;
        }
        Ok(())
    }

    pub(super) async fn expire_cancellations(&mut self) {
        let expired: Vec<_> = self
            .chats
            .iter()
            .filter(|(_, chat)| {
                chat.cancel_deadline
                    .is_some_and(|end| end <= Instant::now())
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Err(error) = self.recover_cancelled(id).await {
                self.events
                    .push(Event::of(id, ChatEvent::Error(error)))
                    .await;
            }
        }
    }
}
