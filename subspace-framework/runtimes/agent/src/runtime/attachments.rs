use super::chat::{Chat, Env};
use super::session::{AccessError, check_access_mode};
use super::startup::{delivery_mode, prepare_prompt};
use super::{ChatEvent, ContextRef, Event};
use crate::ImageAttachment;
use base64::Engine as _;
use github_copilot_sdk::types::{Attachment, DeliveryMode, MessageOptions};

pub(super) fn message_options(
    prompt: String,
    mode: DeliveryMode,
    images: Vec<ImageAttachment>,
) -> MessageOptions {
    let attachments: Vec<Attachment> = images
        .into_iter()
        .map(|image| Attachment::Blob {
            data: base64::engine::general_purpose::STANDARD.encode(image.data.as_ref()),
            mime_type: image.mime_type,
            display_name: Some(image.display_name),
        })
        .collect();
    let message = MessageOptions::new(prompt).with_mode(mode);
    if attachments.is_empty() {
        message
    } else {
        message.with_attachments(attachments)
    }
}

impl Chat {
    pub(super) async fn send_message(
        &mut self,
        env: &Env<'_>,
        text: String,
        context: Vec<ContextRef>,
        images: Vec<ImageAttachment>,
    ) -> Result<(), String> {
        let prompt = prepare_prompt(&text, &context, env.events)?;
        let attachment_ids = images.iter().map(|image| image.id).collect();
        let steering = self.busy;
        // The turn starts before the access check, so the watchdog also sees
        // a check or a send that hangs.
        let attachments = context.len() + images.len();
        self.begin_turn(env, steering, attachments, submitted_texts(&text, &context));
        let message = message_options(prompt, delivery_mode(steering), images);
        let first = !self.persisted;
        if let Err(error) = self.verify_access().await {
            let failure = self.access_failure(env, &error);
            self.send_failed(env, steering, failure).await;
            return Ok(());
        }
        if let Err(error) = self.session.send(message).await {
            let failure = self.send_failure(env, &error);
            self.send_failed(env, steering, failure).await;
            return Ok(());
        }
        env.events
            .push(Event::of(
                self.id,
                ChatEvent::AttachmentsAccepted(attachment_ids),
            ))
            .await;
        self.persisted = true;
        if first {
            self.name_automatically(env, &text).await;
        }
        Ok(())
    }

    /// Checks that the session still has the access mode Subspace set. Until
    /// it passes, the chat refuses to send.
    async fn verify_access(&mut self) -> Result<(), AccessError> {
        self.verified = false;
        check_access_mode(&self.session, self.yolo).await?;
        self.verified = true;
        Ok(())
    }
}

/// The message and each context item's label and content, which a failure
/// must never show or log.
fn submitted_texts(text: &str, context: &[ContextRef]) -> Vec<String> {
    std::iter::once(text.to_owned())
        .chain(
            context
                .iter()
                .flat_map(|item| [item.label.clone(), item.content.clone()]),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::message_options;
    use crate::ImageAttachment;
    use github_copilot_sdk::types::{Attachment, DeliveryMode};

    #[test]
    fn native_blob_is_created_only_from_draft_bytes() {
        let message = message_options(
            "describe".into(),
            DeliveryMode::Immediate,
            vec![ImageAttachment {
                id: 1,
                data: vec![0, 1, 2].into(),
                mime_type: "image/png".into(),
                display_name: "paste.png".into(),
            }],
        );
        let Some(Attachment::Blob {
            data, mime_type, ..
        }) = message.attachments.as_ref().and_then(|a| a.first())
        else {
            panic!("expected native blob attachment");
        };
        assert_eq!(data, "AAEC");
        assert_eq!(mime_type, "image/png");
    }

    #[test]
    fn removed_draft_produces_no_sdk_attachments() {
        let message = message_options("text".into(), DeliveryMode::Immediate, Vec::new());
        assert!(message.attachments.is_none());
    }
}
