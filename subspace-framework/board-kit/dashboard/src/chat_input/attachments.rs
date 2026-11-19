//! Bounded clipboard and path capture. File reads and runtime delivery belong to the host.
use gpui_kit::{ClipboardEntry, ClipboardItem};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use subspace_agent_model::{DRAFT_ATTACHMENT_LIMIT, ImageAttachment};

pub const MAX_DRAFT_IMAGES: usize = 16;
pub const ATTACHMENT_LIMIT_ERROR: &str =
    "Image drafts are limited to 16 files and 20 MiB total. Remove an image first.";

#[derive(Clone)]
pub enum AttachmentSource {
    Path(PathBuf),
    ClipboardImage {
        bytes: Arc<[u8]>,
        mime_type: &'static str,
        display_name: String,
    },
}

#[derive(Clone)]
#[non_exhaustive]
pub struct IngestionBatch {
    pub sources: Vec<AttachmentSource>,
    pub notice: Option<String>,
}

impl fmt::Debug for IngestionBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IngestionBatch")
            .field("source_count", &self.sources.len())
            .field("has_notice", &self.notice.is_some())
            .finish()
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct AttachmentBudget {
    pub remaining_images: usize,
    pub remaining_bytes: usize,
    image_limit: usize,
}

impl AttachmentBudget {
    pub fn new(remaining_images: usize, remaining_bytes: usize) -> Self {
        Self {
            remaining_images,
            remaining_bytes,
            image_limit: crate::images::MAX_ENCODED_BYTES,
        }
    }

    pub fn with_image_limit(mut self, bytes: usize) -> Self {
        self.image_limit = bytes;
        self
    }

    pub fn empty_draft() -> Self {
        Self {
            remaining_images: MAX_DRAFT_IMAGES,
            remaining_bytes: DRAFT_ATTACHMENT_LIMIT,
            image_limit: crate::images::MAX_ENCODED_BYTES,
        }
    }

    pub fn from_images(images: &[ImageAttachment]) -> Self {
        let used_bytes = images.iter().fold(0usize, |total, image| {
            total.saturating_add(image.data.len())
        });
        Self {
            remaining_images: MAX_DRAFT_IMAGES.saturating_sub(images.len()),
            remaining_bytes: DRAFT_ATTACHMENT_LIMIT.saturating_sub(used_bytes),
            image_limit: crate::images::MAX_ENCODED_BYTES,
        }
    }

    pub fn accepts(self, bytes: usize) -> bool {
        self.remaining_images > 0 && self.remaining_bytes > 0 && bytes <= self.remaining_bytes
    }

    pub fn consume(&mut self, bytes: usize) {
        self.remaining_images = self.remaining_images.saturating_sub(1);
        self.remaining_bytes = self.remaining_bytes.saturating_sub(bytes);
    }
}

pub fn path_batch(paths: impl IntoIterator<Item = PathBuf>) -> IngestionBatch {
    let mut paths = paths.into_iter();
    let sources = paths
        .by_ref()
        .take(MAX_DRAFT_IMAGES)
        .map(AttachmentSource::Path)
        .collect();
    let notice = paths.next().map(|_| ATTACHMENT_LIMIT_ERROR.into());
    IngestionBatch { sources, notice }
}

pub fn clipboard_batch(item: &ClipboardItem, budget: AttachmentBudget) -> IngestionBatch {
    clipboard_batch_with(item, budget, |bytes| Arc::<[u8]>::from(bytes))
}

pub fn clipboard_batch_with(
    item: &ClipboardItem,
    mut budget: AttachmentBudget,
    clone_bytes: impl Fn(&[u8]) -> Arc<[u8]>,
) -> IngestionBatch {
    let mut sources = Vec::with_capacity(MAX_DRAFT_IMAGES);
    let mut notice = None;
    for entry in item.entries() {
        match entry {
            ClipboardEntry::Image(image) if sources.len() < MAX_DRAFT_IMAGES => {
                if budget.accepts(image.bytes.len()) && image.bytes.len() <= budget.image_limit {
                    budget.consume(image.bytes.len());
                    sources.push(AttachmentSource::ClipboardImage {
                        bytes: clone_bytes(&image.bytes),
                        mime_type: image.format.mime_type(),
                        display_name: format!("pasted-image.{}", image.format.extension()),
                    });
                } else {
                    notice = Some(ATTACHMENT_LIMIT_ERROR.into());
                }
            }
            ClipboardEntry::ExternalPaths(paths) => {
                let available = MAX_DRAFT_IMAGES.saturating_sub(sources.len());
                sources.extend(
                    paths
                        .0
                        .iter()
                        .take(available)
                        .cloned()
                        .map(AttachmentSource::Path),
                );
                if paths.0.len() > available {
                    notice = Some(ATTACHMENT_LIMIT_ERROR.into());
                }
            }
            ClipboardEntry::Image(_) => notice = Some(ATTACHMENT_LIMIT_ERROR.into()),
            ClipboardEntry::String(_) => {}
        }
    }
    IngestionBatch { sources, notice }
}

impl AttachmentSource {
    pub fn is_image(&self) -> bool {
        match self {
            Self::Path(path) => image_mime(path).is_some(),
            Self::ClipboardImage { .. } => true,
        }
    }
}

pub fn image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{AttachmentBudget, clipboard_batch};
    use gpui_kit::{ClipboardEntry, ClipboardItem, Image, ImageFormat};
    #[test]
    fn service_clipboard_limit_can_increase_without_changing_chat_defaults() {
        let bytes = crate::images::MAX_ENCODED_BYTES + 1;
        let item = ClipboardItem {
            entries: vec![ClipboardEntry::Image(Image::from_bytes(
                ImageFormat::Png,
                vec![0; bytes],
            ))],
        };
        assert!(
            clipboard_batch(&item, AttachmentBudget::new(8, 32 * 1024 * 1024))
                .sources
                .is_empty()
        );
        let batch = clipboard_batch(
            &item,
            AttachmentBudget::new(8, 32 * 1024 * 1024).with_image_limit(10 * 1024 * 1024),
        );
        assert_eq!(batch.sources.len(), 1);
        assert!(batch.notice.is_none());
    }
}
