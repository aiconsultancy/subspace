//! Memory-only attachment intake and explicit recovery; no upload is replayed.
mod intake;
mod recovery;
mod worker;
use super::{CommentDraft, DraftKey};
use crate::{
    hub::{Hub, ScopeGeneration, lock},
    model::IssueMedia,
    uploads::{ImageDeclaration, UploadError},
};
pub use intake::ImageIntake;
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub(super) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
/// A fresh ID for an attachment row, shared with the Attachments panel.
pub(in crate::hub) fn next_attachment_id() -> String {
    next_id().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentState {
    Preparing,
    Uploading,
    Checking,
    NotFound,
    Ready,
    Failed(UploadError),
}

#[derive(Clone)]
pub struct Attachment {
    inner: Arc<Mutex<Row>>,
    running: Arc<AtomicBool>,
}
struct Row {
    id: String,
    /// The comment draft's NETT media draft.
    media_draft: String,
    /// The NETT reservation's request key, once an upload has started.
    request_key: Option<String>,
    declaration: ImageDeclaration,
    bytes: Arc<[u8]>,
    state: AttachmentState,
    media: Option<IssueMedia>,
    existing: Option<HashSet<String>>,
    uncertain: bool,
    checked_absent: bool,
    removed: bool,
    retry_at: Option<Instant>,
}
impl std::fmt::Debug for Attachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Attachment([redacted])")
    }
}
impl Attachment {
    pub fn id(&self) -> String {
        lock(&self.inner).id.clone()
    }
    pub fn file_name(&self) -> String {
        lock(&self.inner).declaration.file_name.clone()
    }
    pub fn state(&self) -> AttachmentState {
        lock(&self.inner).state.clone()
    }
    pub fn size_bytes(&self) -> usize {
        lock(&self.inner).declaration.size_bytes
    }
    pub fn transferred_bytes(&self) -> usize {
        if self.state() == AttachmentState::Ready {
            self.size_bytes()
        } else {
            0
        }
    }
    pub fn retry_ready(&self) -> bool {
        let row = lock(&self.inner);
        !self.running.load(Ordering::Acquire)
            && !row.removed
            && matches!(row.state, AttachmentState::Failed(error) if error.retryable())
            && row.retry_at.is_none_or(|at| Instant::now() >= at)
    }
    fn block(&self) -> Option<String> {
        let row = lock(&self.inner);
        let media = row
            .media
            .as_ref()
            .filter(|_| row.state == AttachmentState::Ready && !row.removed)?;
        let marker = if row.declaration.is_image() { "!" } else { "" };
        Some(format!(
            "{marker}[{}](nett-media:{})",
            if marker.is_empty() {
                "Attached file"
            } else {
                "Attached image"
            },
            media.media_id
        ))
    }
}
impl CommentDraft {
    pub fn attachments(&self) -> &[Attachment] {
        &self.images
    }
    pub fn attachments_ready(&self) -> bool {
        self.intakes == 0
            && self
                .images
                .iter()
                .all(|image| image.state() == AttachmentState::Ready)
    }
    pub fn has_attachments(&self) -> bool {
        !self.images.is_empty() || self.intakes > 0
    }
    pub fn has_pending_images(&self) -> bool {
        self.intakes > 0
    }
    pub(super) fn bound_body(&self) -> String {
        let mut body = self.body().to_string();
        for image in &self.images {
            if let Some(block) = image.block() {
                body.push_str("\n\n");
                body.push_str(&block);
            }
        }
        body
    }
}
impl Hub {
    pub(in crate::hub) fn retire_comment_uploads(&self) {
        let state = lock(&self.0.state);
        for (key, draft) in state.drafts.iter().filter(|(_, draft)| !draft.is_frozen()) {
            for image in draft.attachments() {
                lock(&image.inner).removed = true;
                worker::start(
                    self.0.clone(),
                    state.snapshot.generation,
                    key.clone(),
                    image.clone(),
                );
            }
        }
    }

    fn attach_image_at(
        &self,
        generation: ScopeGeneration,
        target: (&DraftKey, u64),
        declaration: ImageDeclaration,
        bytes: Arc<[u8]>,
    ) -> Result<String, UploadError> {
        let mut state = lock(&self.0.state);
        if !state.current(generation) {
            return Err(UploadError::Removed);
        }
        let (total, cancels) = state.room_for(bytes.len(), Instant::now());
        self.0.cancel_reservations(cancels);
        let draft = state
            .drafts
            .get_mut(target.0)
            .filter(|d| d.lease == target.1 && !d.is_frozen())
            .ok_or(UploadError::Removed)?;
        if let Some(image) = draft
            .images
            .iter()
            .find(|image| lock(&image.inner).declaration == declaration)
        {
            return Ok(image.id());
        }
        let used: usize = draft.images.iter().map(Attachment::size_bytes).sum();
        if draft.images.len() >= crate::uploads::MAX_DRAFT_IMAGES
            || used.saturating_add(bytes.len()) > crate::uploads::MAX_DRAFT_BYTES
            || total.saturating_add(bytes.len()) > crate::uploads::MAX_TOTAL_BYTES
        {
            return Err(UploadError::Budget);
        }
        let buffer = Arc::downgrade(&bytes);
        let image = Attachment {
            inner: Arc::new(Mutex::new(Row {
                id: next_id().to_string(),
                media_draft: draft.media_draft.clone(),
                request_key: None,
                declaration,
                bytes,
                state: AttachmentState::Preparing,
                media: None,
                existing: None,
                uncertain: false,
                checked_absent: false,
                removed: false,
                retry_at: None,
            })),
            running: Arc::new(AtomicBool::new(false)),
        };
        let id = image.id();
        draft.images.push(image.clone());
        state.attachment_buffers.push(buffer);
        state.changed();
        worker::start(self.0.clone(), generation, target.0.clone(), image);
        Ok(id)
    }
    pub fn retry_comment_image(&self, issue: &str, parent: Option<&str>, id: &str) {
        let state = lock(&self.0.state);
        let key = DraftKey::new(issue, parent);
        let image = state
            .drafts
            .get(&key)
            .filter(|d| !d.is_frozen())
            .and_then(|d| d.images.iter().find(|i| i.id() == id && i.retry_ready()))
            .cloned();
        if let Some(image) = image {
            worker::start(self.0.clone(), state.snapshot.generation, key, image);
        }
    }
    pub fn remove_comment_image(&self, issue: &str, parent: Option<&str>, id: &str) {
        let mut state = lock(&self.0.state);
        let key = DraftKey::new(issue, parent);
        let Some(image) = state
            .drafts
            .get(&key)
            .filter(|d| !d.is_frozen())
            .and_then(|d| d.images.iter().find(|i| i.id() == id))
            .cloned()
        else {
            return;
        };
        {
            let mut row = lock(&image.inner);
            row.removed = true;
            row.state = AttachmentState::Checking;
        }
        state.changed();
        worker::start(self.0.clone(), state.snapshot.generation, key, image);
    }
}
