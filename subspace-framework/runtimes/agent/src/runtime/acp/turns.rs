//! Prompt turns: sending a message, stopping it, and its end.
use super::chat::{AcpChat, Pending};
use super::context::{self, Delivery};
use super::protocol::{self, RpcError};
use super::{ChatEvent, Event, EventQueue, Value, json};
use crate::runtime::names::{NEW_CHAT, clean_title};
use crate::{ContextRef, ImageAttachment};
use base64::Engine as _;

impl AcpChat {
    pub(super) async fn send(
        &mut self,
        text: &str,
        context: &[ContextRef],
        images: &[ImageAttachment],
        events: &EventQueue,
    ) -> Result<(), String> {
        let session = self.session_for_prompt()?;
        if !images.is_empty() && !self.capabilities.images {
            return Err(format!("{} does not accept images.", self.agent));
        }
        let message = crate::runtime::prompts::prepare_prompt(text, context, events)?;
        let blocks = prompt_blocks(self.pending_context(), &message, images);
        let outputs = self.transcript.user(&message);
        self.emit(outputs, events).await;
        self.request(
            Pending::Prompt,
            "session/prompt",
            json!({"sessionId": session, "prompt": blocks}),
        )
        .await?;
        self.context_sent();
        self.busy = true;
        self.cancelled = false;
        if !images.is_empty() {
            let accepted = images.iter().map(|image| image.id).collect();
            events
                .push(Event::of(self.id, ChatEvent::AttachmentsAccepted(accepted)))
                .await;
        }
        if self.title == NEW_CHAT {
            self.retitle(clean_title(text).as_deref(), events).await;
        }
        Ok(())
    }

    /// The context block for a new session's first prompt, while it is due.
    fn pending_context(&self) -> Option<Value> {
        self.context
            .as_deref()
            .filter(|_| self.context_pending)
            .map(context::prompt_block)
    }

    fn context_sent(&mut self) {
        if let Some(text) = self.context.as_deref().filter(|_| self.context_pending) {
            context::log_delivery(self.id, Delivery::FirstPrompt, text);
        }
        self.context_pending = false;
    }

    fn session_for_prompt(&self) -> Result<String, String> {
        let agent = self.agent;
        if self.busy {
            return Err(format!(
                "{agent} is still answering. Wait for it, or press Stop."
            ));
        }
        self.session_id
            .clone()
            .filter(|_| self.ready)
            .ok_or_else(|| format!("{agent} is still starting. Try again shortly."))
    }

    /// Gives the agent a bounded chance to finish its prompt after Stop.
    pub(super) async fn cancel(&mut self) -> Result<(), String> {
        if !self.busy || self.cancel_deadline.is_some() {
            return Ok(());
        }
        self.cancelled = true;
        let deadline = tokio::time::Instant::now() + super::cancellation::CANCEL_GRACE_PERIOD;
        self.cancel_deadline = Some(deadline);
        tokio::time::timeout_at(deadline, self.send_cancel())
            .await
            .map_err(|_| "ACP cancellation timed out.".to_owned())?
    }

    async fn send_cancel(&mut self) -> Result<(), String> {
        let session = self.session_id.clone().unwrap_or_default();
        self.write(protocol::notification(
            "session/cancel",
            json!({"sessionId": session}),
        ))
        .await?;
        for (id, _) in std::mem::take(&mut self.permissions) {
            self.write(protocol::reply(&id, protocol::permission_outcome(None)))
                .await?;
        }
        Ok(())
    }

    pub(super) async fn turn_ended(
        &mut self,
        result: Result<&Value, RpcError>,
        events: &EventQueue,
    ) -> Result<(), String> {
        self.busy = false;
        self.cancel_deadline = None;
        let stopped = result
            .as_ref()
            .map_or(true, |result| protocol::cancelled(result));
        let finished = self.transcript.finish(stopped || self.cancelled);
        self.emit(finished, events).await;
        let problem = match &result {
            Ok(result) => {
                let usage = protocol::usage(result);
                events
                    .push(Event::of(self.id, ChatEvent::Usage(usage)))
                    .await;
                protocol::stop_problem(result).map(str::to_owned)
            }
            Err(error) if error.needs_sign_in() => Some(self.failure(&Pending::Prompt, error)),
            Err(error) => Some(format!(
                "{} stopped the turn: {}",
                self.agent, error.message
            )),
        };
        if let Some(problem) = problem {
            events
                .push(Event::of(self.id, ChatEvent::Error(problem)))
                .await;
        }
        events.push(Event::of(self.id, ChatEvent::TurnDone)).await;
        Ok(())
    }
}

/// The prompt's content blocks: Subspace's context when it leads this
/// prompt, the message, then each image.
fn prompt_blocks(lead: Option<Value>, message: &str, images: &[ImageAttachment]) -> Vec<Value> {
    let text = json!({"type": "text", "text": message});
    let images = images.iter().map(|image| {
        let data = base64::engine::general_purpose::STANDARD.encode(&image.data);
        json!({"type": "image", "mimeType": image.mime_type, "data": data})
    });
    lead.into_iter()
        .chain(std::iter::once(text))
        .chain(images)
        .collect()
}
