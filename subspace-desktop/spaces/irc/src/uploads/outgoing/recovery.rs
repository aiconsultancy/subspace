//! "Check upload" for an upload that may have completed: asks the API with
//! the same stable ID instead of uploading again.
use super::error;
use crate::conversation::Conversation;
use crate::uploads::session::Scope;
use gpui_kit::{AppContext as _, Context};
use subspace_api_client::chat_uploads::{Upload, UploadError};

pub(crate) fn check(this: &mut Conversation, id: &str, cx: &mut Context<Conversation>) {
    if !this.tray.idle() {
        return;
    }
    let scope = this.hub.update(cx, |hub, _| {
        hub.uploads.demand.activate();
        hub.uploads.session.scope()
    });
    let Some(scope) = scope else {
        error(this, &UploadError::Access.to_string(), cx);
        return;
    };
    let Some(p) = this.tray.pending_mut(id) else {
        return;
    };
    if p.busy {
        return;
    }
    if !p
        .scope
        .as_ref()
        .is_some_and(|original| original.same_user(&scope))
    {
        p.error = Some(UploadError::Access);
        cx.notify();
        return;
    }
    p.busy = true;
    let id = id.to_owned();
    let draft = p.draft.clone();
    cx.spawn(async move |this, cx| {
        let read_scope = scope.clone();
        let result = cx
            .background_spawn(async move { read_scope.client()?.check_chat_upload(&draft) })
            .await;
        let _ = this.update(cx, |this, cx| checked(this, &id, scope, result, cx));
    })
    .detach();
    cx.notify();
}

/// Records what the API said about an unclear upload.
fn checked(
    this: &mut Conversation,
    id: &str,
    scope: Scope,
    result: Result<Upload, UploadError>,
    cx: &mut Context<Conversation>,
) {
    let authorised = this.hub.read(cx).uploads.session.matches(&scope);
    let Some(p) = this.tray.pending_mut(id) else {
        return;
    };
    p.busy = false;
    match result {
        _ if !authorised => p.error = Some(UploadError::Access),
        Ok(upload) => {
            p.scope = Some(scope);
            p.uploaded = Some(upload);
            p.uncertain = false;
            p.error = None;
        }
        Err(UploadError::Missing) => {
            // Not there: the next explicit send uploads it afresh.
            p.forget_upload();
            p.uncertain = false;
            p.error = Some(UploadError::Missing);
        }
        Err(e) => p.error = Some(e),
    }
    cx.notify();
}
