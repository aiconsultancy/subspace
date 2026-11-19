//! Files into the tray: drops and pastes take the first ones up to the
//! limits and say when some were left out.
use super::super::intake;
use super::{ItemState, LIMIT_NOTICE, changed, hold, pending};
use crate::conversation::Conversation;
use gpui_kit::{AppContext as _, Context};
use subspace_framework::dashboard::chat_input::attachments::{AttachmentSource, IngestionBatch};

/// Dropped files.
pub(crate) fn paths(
    this: &mut Conversation,
    paths: Vec<std::path::PathBuf>,
    cx: &mut Context<Conversation>,
) {
    let sources = paths.into_iter().map(AttachmentSource::Path).collect();
    add(this, sources, None, cx);
}

/// Pasted files and images.
pub(crate) fn ingest(
    this: &mut Conversation,
    batch: IngestionBatch,
    cx: &mut Context<Conversation>,
) {
    // The paste budget's image limit is the tray total, so every notice the
    // clipboard reports is a count or total overflow: the tray's notice.
    let notice = batch.notice.is_some().then_some(LIMIT_NOTICE);
    add(this, batch.sources, notice, cx);
}

fn add(
    this: &mut Conversation,
    sources: Vec<AttachmentSource>,
    notice: Option<&'static str>,
    cx: &mut Context<Conversation>,
) {
    if !this.tray.idle() {
        super::error(this, "Wait for the attachments to send first.", cx);
        return;
    }
    this.tray.notice = notice;
    if !sources.is_empty() {
        hold(this, cx);
    }
    for source in sources {
        let Some(id) = this.tray.push(ItemState::Preparing(None), None) else {
            break;
        };
        prepare(id, source, cx);
    }
    changed(this, cx);
}

fn prepare(id: usize, source: AttachmentSource, cx: &mut Context<Conversation>) {
    cx.spawn(async move |this, cx| {
        let result = cx
            .background_spawn(async move { intake::prepare(source) })
            .await;
        let _ = this.update(cx, |this, cx| {
            let Some(item) = this.tray.items.iter_mut().find(|item| item.id == id) else {
                return;
            };
            if !matches!(item.state, ItemState::Preparing(None)) {
                return;
            }
            item.state = match result {
                Ok((draft, preview)) => ItemState::Preparing(Some(pending(draft, preview))),
                Err(e) => ItemState::Failed(e),
            };
            this.tray.admit();
            changed(this, cx);
        });
    })
    .detach();
}
