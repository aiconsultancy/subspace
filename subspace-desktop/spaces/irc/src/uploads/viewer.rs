mod io;
use io::read;
mod saving;
use saving::save;
mod view;
use super::intake::Preview;
use crate::conversation::Conversation;
use gpui_kit::{
    App, AppContext as _, Context, FocusHandle, KeyBinding, WeakEntity, Window, actions,
};
use std::sync::Arc;
use subspace_api_client::chat_uploads::{Upload, UploadError};
pub(crate) use view::render;
const CONTEXT: &str = "ChatUploadViewer";
actions!(chat_upload_viewer, [CloseUpload]);
pub(crate) type Claim = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;
pub(crate) struct Viewer {
    _lease: Arc<()>,
    scope: Option<super::session::Scope>,
    pub(crate) key: String,
    metadata: Option<Upload>,
    content: Option<Preview>,
    bytes: Option<Arc<[u8]>>,
    error: Option<UploadError>,
    previous: Option<FocusHandle>,
    close: FocusHandle,
    ticket: u64,
    save_error: Option<String>,
    saving: bool,
    scroll: gpui_kit::ScrollHandle,
}
pub(crate) fn bind(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseUpload, Some(CONTEXT)),
        KeyBinding::new(
            "escape",
            subspace_framework::dashboard::find_in_page::Close,
            Some("ChatUploadViewer && !SubspaceModal && !SubspaceRawKeys"),
        ),
    ]);
}
pub(crate) fn claim(owner: &WeakEntity<Conversation>) -> Claim {
    let owner = owner.clone();
    Arc::new(move |key, window, cx| {
        let _ = owner.update(cx, |this, cx| open(this, key.to_owned(), window, cx));
    })
}
pub(crate) fn open(
    this: &mut Conversation,
    key: String,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let ticket = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let lease = this.hub.update(cx, |hub, _| hub.uploads.demand.hold());
    let previous = this
        .upload_viewer
        .as_ref()
        .map_or_else(|| window.focused(cx), |v| v.previous.clone());
    let close = cx.focus_handle().tab_stop(true);
    close.focus(window, cx);
    let scope = this.hub.read(cx).uploads.session.scope();
    this.upload_viewer = Some(Viewer {
        _lease: lease,
        scope: scope.clone(),
        key: key.clone(),
        metadata: None,
        content: None,
        bytes: None,
        error: None,
        previous,
        close,
        ticket,
        save_error: None,
        saving: false,
        scroll: gpui_kit::ScrollHandle::new(),
    });
    cx.spawn(async move |this, cx| {
        let requested = key.clone();
        let result = cx
            .background_spawn(async move { read(&requested, scope.as_ref()) })
            .await;
        let _ = this.update(cx, |this, cx| {
            if let Some(viewer) = this.upload_viewer.as_mut().filter(|v| v.ticket == ticket) {
                match result {
                    Ok((meta, bytes, content)) => {
                        viewer.metadata = Some(meta);
                        viewer.bytes = bytes;
                        viewer.content = Some(content);
                    }
                    Err(e) => viewer.error = Some(e),
                }
                cx.notify();
            }
        });
    })
    .detach();
    cx.emit(crate::conversation::ConversationEvent::FindScopeChanged);
    cx.notify();
}

fn close(this: &mut Conversation, window: &mut Window, cx: &mut Context<Conversation>) {
    if let Some(viewer) = this.upload_viewer.take() {
        if let Some(focus) = viewer.previous {
            focus.focus(window, cx);
        } else {
            this.focus_composer(window, cx);
        }
    }
    cx.emit(crate::conversation::ConversationEvent::FindScopeChanged);
    cx.notify();
}

pub(crate) fn follow(this: &mut Conversation, cx: &mut Context<Conversation>) {
    if let Some(viewer) = &mut this.upload_viewer
        && !viewer
            .scope
            .as_ref()
            .is_some_and(|scope| this.hub.read(cx).uploads.session.matches(scope))
    {
        viewer.metadata = None;
        viewer.bytes = None;
        viewer.content = None;
        viewer.error = Some(UploadError::Access);
    }
}

/// Isolate a displayed attachment from its background transcript during local find.
pub(crate) fn find_scope(this: &Conversation) -> Option<gpui_kit::SharedString> {
    this.upload_viewer
        .as_ref()
        .map(|viewer| format!("chat-upload:{}:{}", viewer.key, viewer.ticket).into())
}
