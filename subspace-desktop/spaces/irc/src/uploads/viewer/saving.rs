use super::super::session::Scope;
use crate::conversation::Conversation;
use gpui_kit::{App, AppContext as _, AsyncApp, Context, WeakEntity};
use std::{path::PathBuf, sync::Arc};
use subspace_api_client::chat_uploads::{Upload, UploadError};

#[derive(Clone)]
struct Request {
    scope: Scope,
    meta: Upload,
    bytes: Option<Arc<[u8]>>,
    ticket: u64,
}
impl Request {
    fn allowed(&self, this: &Conversation, cx: &App) -> bool {
        this.upload_viewer
            .as_ref()
            .is_some_and(|v| v.ticket == self.ticket)
            && this.hub.read(cx).uploads.session.matches(&self.scope)
    }
    fn load(&self) -> Result<Arc<[u8]>, UploadError> {
        let client = self.scope.client()?;
        let bytes = match &self.bytes {
            Some(bytes) => bytes.clone(),
            None => client.chat_upload_bytes(&self.meta)?.into(),
        };
        self.scope.client()?;
        Ok(bytes)
    }
}
fn request(this: &mut Conversation, cx: &mut Context<Conversation>) -> Option<Request> {
    let viewer = this.upload_viewer.as_mut()?;
    let scope = viewer.scope.clone()?;
    if viewer.saving || !this.hub.read(cx).uploads.session.matches(&scope) {
        return None;
    }
    let request = Request {
        scope,
        meta: viewer.metadata.clone()?,
        bytes: viewer.bytes.clone(),
        ticket: viewer.ticket,
    };
    viewer.saving = true;
    viewer.save_error = None;
    Some(request)
}
pub(super) fn save(this: &mut Conversation, cx: &mut Context<Conversation>) {
    let Some(request) = request(this, cx) else {
        return;
    };
    let ticket = request.ticket;
    let picker = cx.prompt_for_new_path(std::path::Path::new("."), Some(&request.meta.file_name));
    cx.notify();
    cx.spawn(async move |this, cx| {
        let result = match picker.await {
            Ok(Ok(Some(path))) => write(&this, cx, request, path).await,
            _ => Ok(()),
        };
        let _ = this.update(cx, |this, cx| {
            if let Some(viewer) = this.upload_viewer.as_mut().filter(|v| v.ticket == ticket) {
                viewer.saving = false;
                viewer.save_error = result.err();
            }
            cx.notify();
        });
    })
    .detach();
}
async fn write(
    this: &WeakEntity<Conversation>,
    cx: &mut AsyncApp,
    request: Request,
    path: PathBuf,
) -> Result<(), String> {
    if !this
        .update(cx, |this, cx| request.allowed(this, cx))
        .unwrap_or(false)
    {
        return Err(UploadError::Access.to_string());
    }
    let loading = request.clone();
    let bytes = cx
        .background_spawn(async move { loading.load() })
        .await
        .map_err(|e| e.to_string())?;
    let allowed = this
        .update(cx, |this, cx| request.allowed(this, cx))
        .unwrap_or(false);
    if !allowed {
        return Err(UploadError::Access.to_string());
    }
    cx.background_spawn(async move { std::fs::write(path, bytes) })
        .await
        .map_err(|_| "Couldn’t save the file. Choose another location.".to_owned())
}
