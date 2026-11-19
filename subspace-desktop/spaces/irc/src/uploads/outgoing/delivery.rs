//! Sending the tray: upload every file in tray order, then hand over to
//! `send`. One failed upload sends nothing to IRC.
use super::{ItemState, Phase, error};
use crate::conversation::Conversation;
use gpui_kit::{App, AppContext as _, Context, Window};
use subspace_api_client::chat_uploads::{self, UploadError};
use subspace_client_irc::{ConnectionStatus, Target};

pub(super) fn sendable(this: &Conversation, cx: &App) -> bool {
    let hub = this.hub.read(cx);
    let connected = hub
        .state
        .network(&this.buffer.network)
        .is_some_and(|n| matches!(n.status, ConnectionStatus::Connected { .. }));
    connected
        && match &this.buffer.target {
            Target::Channel(_) => hub.state.buffer(&this.buffer).is_some_and(|b| b.joined),
            Target::Query(_) => true,
            Target::Server => false,
        }
}

/// Enter with files in the tray. Returns `false` when the tray is empty, so
/// the composer sends `text` as usual.
pub(crate) fn submit(
    this: &mut Conversation,
    text: String,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) -> bool {
    if this.tray.is_empty() {
        return false;
    }
    if let Phase::Queued { text: queued, .. } = &this.tray.phase {
        // A queued send never locks the conversation: new text goes as a
        // plain line. Enter again with the queued text, or nothing, sends
        // nothing: the queued tray and its text are never sent again.
        let text = text.trim();
        return text.is_empty() || text == queued.trim();
    }
    if !this.tray.idle() {
        return true;
    }
    if let Some(problem) = blocker(this, &text, cx) {
        error(this, problem, cx);
        return true;
    }
    for item in &mut this.tray.items {
        if let ItemState::Ready(p) = &mut item.state {
            p.error = None;
        }
    }
    this.tray.phase = Phase::Uploading { text };
    step(this, window, cx);
    true
}

/// Why the tray cannot send now, if it cannot.
fn blocker(this: &Conversation, text: &str, cx: &App) -> Option<&'static str> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('/') && !trimmed.starts_with("//") {
        return Some("Send commands without attachments.");
    }
    if !sendable(this, cx) {
        return Some("Connect to the conversation before sending attachments.");
    }
    this.tray.items.iter().find_map(|item| match &item.state {
        ItemState::Preparing(_) => Some(super::PREPARING),
        ItemState::Failed(_) => Some("Remove the attachment that couldn’t be read."),
        ItemState::Ready(p) if p.uncertain => Some("Check the upload before sending again."),
        ItemState::Ready(_) => None,
    })
}

/// Uploads the next file, or sends once every file is uploaded.
fn step(this: &mut Conversation, window: &mut Window, cx: &mut Context<Conversation>) {
    let scope = this.hub.update(cx, |hub, _| {
        hub.uploads.demand.activate();
        hub.uploads.session.scope()
    });
    let Some(scope) = scope else {
        stop(this, UploadError::Access, cx);
        return;
    };
    let next = this
        .tray
        .items
        .iter_mut()
        .find_map(|item| match &mut item.state {
            ItemState::Ready(p) if p.uploaded.is_none() => Some(p),
            _ => None,
        });
    let Some(p) = next else {
        super::send::queue(this, window, cx);
        return;
    };
    p.scope = Some(scope.clone());
    p.busy = true;
    let draft = p.draft.clone();
    let id = draft.id().to_owned();
    cx.spawn_in(window, async move |this, cx| {
        let read_scope = scope.clone();
        let result = cx
            .background_spawn(async move { read_scope.client()?.create_chat_upload(&draft) })
            .await;
        let _ = this.update_in(cx, |this, window, cx| {
            uploaded(this, (&id, &scope), result, window, cx);
        });
    })
    .detach();
    cx.notify();
}

fn uploaded(
    this: &mut Conversation,
    (id, scope): (&str, &super::super::session::Scope),
    result: Result<chat_uploads::Upload, UploadError>,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    let authorised = this.hub.read(cx).uploads.session.matches(scope);
    let Some(p) = this.tray.pending_mut(id) else {
        return;
    };
    p.busy = false;
    let failure = match result {
        _ if !authorised => {
            p.uncertain = true;
            UploadError::Access
        }
        Ok(upload) => {
            p.uploaded = Some(upload);
            step(this, window, cx);
            return;
        }
        Err(e @ (UploadError::Unconfirmed | UploadError::Conflict)) => {
            p.uncertain = true;
            e
        }
        Err(e) => e,
    };
    p.error = Some(failure.clone());
    stop(this, failure, cx);
}

/// Ends the send without reaching IRC; the tray and draft stay for a retry.
pub(super) fn stop(this: &mut Conversation, failure: UploadError, cx: &mut Context<Conversation>) {
    this.tray.phase = Phase::Idle;
    error(this, &failure.to_string(), cx);
}
