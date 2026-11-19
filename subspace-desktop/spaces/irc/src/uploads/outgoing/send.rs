//! Sending the line: once every file uploaded in this session, the text
//! and every reference go to IRC in as few lines as fit, and the tray
//! waits for their echo.
use super::{ItemState, Phase, error};
use crate::conversation::Conversation;
use gpui_kit::{Context, Window};
use subspace_api_client::chat_uploads::{self, UploadError};
use subspace_client_irc::Command;

/// Every upload, once each is ready to send in this session. An upload
/// made before the session changed becomes unclear: Check upload confirms
/// it with the same ID, so nothing uploads or sends again by itself.
fn ready_uploads(
    this: &mut Conversation,
    cx: &mut Context<Conversation>,
) -> Result<Vec<chat_uploads::Upload>, UploadError> {
    let session = &this.hub.read(cx).uploads.session;
    let mut uploads = Vec::new();
    let mut failure = None;
    for item in &mut this.tray.items {
        let ItemState::Ready(p) = &mut item.state else {
            failure.get_or_insert(UploadError::Unavailable);
            continue;
        };
        let (Some(upload), Some(scope)) = (&p.uploaded, &p.scope) else {
            failure.get_or_insert(UploadError::Unavailable);
            continue;
        };
        if !session.matches(scope) {
            p.uncertain = true;
            p.error = Some(UploadError::Unconfirmed);
            failure.get_or_insert(UploadError::Unconfirmed);
        } else if upload.expired() {
            // Marks the file; the next explicit send uploads it afresh.
            p.forget_upload();
            p.error = Some(UploadError::Expired);
            failure.get_or_insert(UploadError::Expired);
        } else {
            uploads.push(upload.clone());
        }
    }
    failure.map_or(Ok(uploads), Err)
}

/// Sends the text and every reference, in as few lines as fit.
pub(super) fn queue(this: &mut Conversation, window: &mut Window, cx: &mut Context<Conversation>) {
    let Phase::Uploading { text } = std::mem::take(&mut this.tray.phase) else {
        return;
    };
    if !super::delivery::sendable(this, cx) {
        error(this, "Uploaded. Reconnect, then press Enter to send.", cx);
        return;
    }
    let uploads = match ready_uploads(this, cx) {
        Ok(uploads) => uploads,
        Err(failure) => return super::delivery::stop(this, failure, cx),
    };
    let budget = crate::hub::message_budget(this.hub.read(cx), &this.buffer);
    let references: Vec<String> = uploads
        .iter()
        .map(|upload| chat_uploads::reference_text(upload, budget))
        .collect();
    // One `SendAll`: the runtime checks registration and every line against
    // the negotiated limits before writing any, and answers once the write
    // ended. A written line is not proof the server accepted it.
    let id = this.hub.update(cx, |hub, _| hub.state.next_send());
    this.send(
        Command::SendAll {
            buffer: this.buffer.clone(),
            id,
            lines: chat_uploads::lines(&text, &references, budget),
        },
        cx,
    );
    let keys = uploads.into_iter().map(|upload| upload.reference).collect();
    this.tray.phase = Phase::Queued {
        text,
        keys,
        window: window.window_handle(),
        id,
        accepted: false,
    };
    this.focus_composer(window, cx);
    cx.notify();
}
