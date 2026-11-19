use super::AgentHandle;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use subspace_agent::{ContextRef, DRAFT_ATTACHMENT_LIMIT, ImageAttachment};
use subspace_dashboard::chat_input::attachments::{ATTACHMENT_LIMIT_ERROR, image_mime};
pub(super) use subspace_dashboard::chat_input::attachments::{
    AttachmentBudget, AttachmentSource, IngestionBatch, MAX_DRAFT_IMAGES, path_batch,
};
#[cfg(test)]
pub(super) use subspace_dashboard::chat_input::attachments::{
    clipboard_batch, clipboard_batch_with,
};

static NEXT_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

pub(super) enum DroppedAttachment {
    Text(ContextRef),
    Image(ImageAttachment),
}

pub(super) type AttachmentResult = Result<DroppedAttachment, String>;

pub(super) fn ingest_batch(
    sources: Vec<AttachmentSource>,
    mut budget: AttachmentBudget,
    mut ingest: impl FnMut(AttachmentSource, usize) -> AttachmentResult,
) -> Vec<AttachmentResult> {
    let mut results = Vec::with_capacity(sources.len());
    for source in sources.into_iter().take(MAX_DRAFT_IMAGES) {
        if source.is_image() && !budget.accepts(0) {
            results.push(Err(ATTACHMENT_LIMIT_ERROR.into()));
            continue;
        }
        let result = ingest(source, budget.remaining_bytes);
        if let Ok(DroppedAttachment::Image(image)) = &result {
            if !budget.accepts(image.data.len()) {
                results.push(Err(ATTACHMENT_LIMIT_ERROR.into()));
                continue;
            }
            budget.consume(image.data.len());
        }
        results.push(result);
    }
    results
}

pub(super) fn ingest_source(
    handle: Option<&AgentHandle>,
    source: AttachmentSource,
    byte_limit: usize,
) -> AttachmentResult {
    match source {
        AttachmentSource::Path(path) => ingest_path(handle, &path, byte_limit),
        AttachmentSource::ClipboardImage {
            bytes,
            mime_type,
            display_name,
        } => {
            if bytes.len() > byte_limit || bytes.len() > crate::images::MAX_ENCODED_BYTES {
                return Err(ATTACHMENT_LIMIT_ERROR.into());
            }
            validated_image(bytes, mime_type, display_name).map(DroppedAttachment::Image)
        }
    }
}

fn ingest_path(handle: Option<&AgentHandle>, path: &Path, byte_limit: usize) -> AttachmentResult {
    let Some(mime_type) = image_mime(path) else {
        let handle = handle.ok_or_else(|| "Open chat before attaching a file.".to_string())?;
        return handle.attach_file(path).map(DroppedAttachment::Text);
    };
    let read_limit = byte_limit.min(crate::images::MAX_ENCODED_BYTES) as u64;
    let bytes = match handle {
        Some(handle) => handle.read_attachment_image(path, read_limit)?,
        None => subspace_agent::read_attachment_image(path, read_limit)?,
    };
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Attachment needs a UTF-8 file name.".to_string())?;
    validated_image(bytes.into(), mime_type, display_name.into()).map(DroppedAttachment::Image)
}

fn validated_image(
    bytes: Arc<[u8]>,
    mime_type: &str,
    display_name: String,
) -> Result<ImageAttachment, String> {
    crate::images::decode(&bytes, mime_type)?;
    Ok(ImageAttachment {
        id: NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
        data: bytes,
        mime_type: mime_type.into(),
        display_name,
    })
}

pub(super) fn add_image(
    images: &mut Vec<ImageAttachment>,
    image: ImageAttachment,
) -> Result<(), String> {
    let total = images.iter().fold(image.data.len(), |total, current| {
        total.saturating_add(current.data.len())
    });
    if images.len() >= MAX_DRAFT_IMAGES || total > DRAFT_ATTACHMENT_LIMIT {
        return Err(ATTACHMENT_LIMIT_ERROR.into());
    }
    images.push(image);
    Ok(())
}
