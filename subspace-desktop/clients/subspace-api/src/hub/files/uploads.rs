//! Standalone attachments being added: admitted within the shared byte
//! budget, sent on a small pool, kept with their issue while it is closed,
//! and, once their bytes may be stored, completed rather than sent again.
mod parking;
mod sending;

use crate::{
    hub::{AttachmentState, Hub, lock},
    uploads::{ImageDeclaration, UploadError},
};
use std::sync::Arc;

/// A standalone attachment being added. It keeps its request key, the
/// transfer it reserved and whether its bytes may have been stored: once
/// they may have been, any failure is resolved by completing that transfer,
/// never by sending the bytes again. A wait NETT asked for is kept too.
#[derive(Clone)]
pub struct AttachmentUpload {
    id: String,
    request_key: String,
    declaration: ImageDeclaration,
    bytes: Arc<[u8]>,
    state: AttachmentState,
    transfer: Option<String>,
    bytes_sent: bool,
    retry_at: Option<std::time::Instant>,
}

impl AttachmentUpload {
    /// A new upload of `bytes` under a fresh request key, not yet sent.
    fn new(declaration: ImageDeclaration, bytes: Arc<[u8]>) -> Self {
        Self {
            id: crate::hub::comments::next_attachment_id(),
            request_key: uuid::Uuid::new_v4().to_string(),
            declaration,
            bytes,
            state: AttachmentState::Uploading,
            transfer: None,
            bytes_sent: false,
            retry_at: None,
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn file_name(&self) -> &str {
        &self.declaration.file_name
    }
    pub fn state(&self) -> &AttachmentState {
        &self.state
    }
    /// Whether the next step checks the transfer rather than sending: once
    /// a reserved transfer's bytes may have been stored, it is completed,
    /// never sent again.
    pub fn checks(&self) -> bool {
        self.bytes_sent && self.transfer.is_some()
    }
    /// Whether Retry or Check can still change the outcome. A file NETT
    /// rejected (400) or forbade (403) stays refused, so only Dismiss is
    /// offered.
    pub fn retryable(&self) -> bool {
        match self.state {
            AttachmentState::Failed(UploadError::Refused {
                status: 400 | 403 | 413 | 415 | 422,
                ..
            }) => false,
            AttachmentState::Failed(error) => error.retryable(),
            AttachmentState::NotFound => true,
            _ => false,
        }
    }
    /// How long before NETT lets this upload be checked or sent again.
    pub fn retry_delay(&self) -> Option<std::time::Duration> {
        self.retry_at
            .and_then(|at| at.checked_duration_since(std::time::Instant::now()))
    }
}

impl std::fmt::Debug for AttachmentUpload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AttachmentUpload([redacted])")
    }
}

/// Whether one more file of `size` bytes may be added beside `uploads`,
/// with `held` bytes already held by drafts and uploads across the app: at
/// most eight uploads and 32 MiB on one issue, and 64 MiB in all, as for a
/// comment draft. A failed upload keeps its bytes for Retry, so it counts
/// until it is stored or dismissed.
fn admit(uploads: &[AttachmentUpload], held: usize, size: usize) -> Result<(), UploadError> {
    let here: usize = uploads.iter().map(|upload| upload.bytes.len()).sum();
    if uploads.len() >= crate::uploads::MAX_DRAFT_IMAGES
        || here.saturating_add(size) > crate::uploads::MAX_DRAFT_BYTES
        || held.saturating_add(size) > crate::uploads::MAX_TOTAL_BYTES
    {
        return Err(UploadError::Budget);
    }
    Ok(())
}

/// The issue, its opening and the scope a standalone attachment is added
/// to, taken when the person starts choosing a file. Nothing is added if
/// any of them changed, even if the same issue was opened again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentTarget {
    generation: crate::hub::ScopeGeneration,
    issue: String,
    opening: u64,
}

impl Hub {
    /// Where a file chosen now would be added: the open issue in this scope.
    pub fn attachment_target(&self) -> Option<AttachmentTarget> {
        let state = lock(&self.0.state);
        let issue = state
            .snapshot
            .issue
            .as_ref()
            .filter(|i| i.detail.is_some())?;
        Some(AttachmentTarget {
            generation: state.snapshot.generation,
            issue: issue.identifier.clone(),
            opening: issue.opening,
        })
    }

    /// Adds a standalone attachment to the open issue. The upload runs in the
    /// background and shows in the Attachments panel until it is stored.
    #[cfg(any(test, feature = "test-support"))]
    // qual:test_helper
    pub fn add_attachment(
        &self,
        name: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<(), UploadError> {
        let target = self.attachment_target().ok_or(UploadError::Removed)?;
        self.add_attachment_to(&target, name, mime, bytes)
    }

    /// Adds a standalone attachment to `target`, refusing it when the open
    /// issue or the scope is no longer the one the file was chosen for.
    pub fn add_attachment_to(
        &self,
        target: &AttachmentTarget,
        name: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<(), UploadError> {
        let declaration = ImageDeclaration::new(name, mime, &bytes)?;
        let mut state = lock(&self.0.state);
        state.running().map_err(|_| UploadError::Removed)?;
        if !state.current(target.generation) {
            return Err(UploadError::Removed);
        }
        // Every buffer still held, by drafts or uploads, counts against
        // the budget across the app.
        state
            .attachment_buffers
            .retain(|buffer| buffer.strong_count() > 0);
        let held: usize = state
            .attachment_buffers
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .map(|buffer| buffer.len())
            .sum();
        let Some(issue) = state.snapshot.issue.as_mut().filter(|i| {
            i.detail.is_some() && i.identifier == target.issue && i.opening == target.opening
        }) else {
            return Err(UploadError::Removed);
        };
        admit(&issue.files.uploads, held, bytes.len())?;
        let bytes: Arc<[u8]> = bytes.into();
        let buffer = Arc::downgrade(&bytes);
        let upload = AttachmentUpload::new(declaration, bytes);
        issue.files.notice = None;
        issue.files.uploads.push(upload.clone());
        let identifier = issue.identifier.clone();
        let generation = target.generation;
        state.attachment_buffers.push(buffer);
        state.changed();
        let shared = Arc::clone(&self.0);
        sending::run(move || sending::send(&shared, generation, &identifier, &upload));
        Ok(())
    }

    /// Checks an upload whose bytes may have been stored by completing its
    /// transfer, or sends one whose bytes were not, under the same key. A
    /// wait NETT asked for is honoured: nothing is sent before it ends.
    pub fn retry_attachment_upload(&self, id: &str) {
        let mut state = lock(&self.0.state);
        let generation = state.snapshot.generation;
        let Some(issue) = state.snapshot.issue.as_mut() else {
            return;
        };
        let Some(upload) = issue.files.uploads.iter_mut().find(|u| u.id == id) else {
            return;
        };
        if !upload.retryable() {
            return;
        }
        if upload.retry_delay().is_some() {
            issue.files.notice = Some("NETT asked to wait before this upload is checked again.");
            state.changed();
            return;
        }
        upload.state = if upload.checks() {
            AttachmentState::Checking
        } else {
            AttachmentState::Uploading
        };
        let upload = upload.clone();
        let identifier = issue.identifier.clone();
        state.changed();
        let shared = Arc::clone(&self.0);
        sending::run(move || sending::send(&shared, generation, &identifier, &upload));
    }

    /// Stops showing an upload that failed, cancelling its reservation when
    /// it was never completed.
    pub fn dismiss_attachment_upload(&self, id: &str) {
        let mut state = lock(&self.0.state);
        let Some(issue) = state.snapshot.issue.as_mut() else {
            return;
        };
        let Some(index) = issue.files.uploads.iter().position(|u| {
            u.id == id
                && matches!(
                    u.state,
                    AttachmentState::Failed(_) | AttachmentState::NotFound
                )
        }) else {
            return;
        };
        let upload = issue.files.uploads.remove(index);
        let identifier = issue.identifier.clone();
        state.changed();
        if let Some(transfer) = upload.transfer {
            let shared = Arc::clone(&self.0);
            sending::run(move || {
                shared
                    .client
                    .cancel_issue_attachment(&identifier, &transfer)
            });
        }
    }
}

#[cfg(test)]
mod admission_tests {
    use super::{AttachmentUpload, admit};
    use crate::{hub::AttachmentState, uploads::UploadError};

    pub(super) fn upload(size: usize) -> AttachmentUpload {
        let bytes = vec![b'a'; size];
        AttachmentUpload {
            id: "u".into(),
            request_key: "k".into(),
            declaration: crate::uploads::ImageDeclaration::new("a.txt", "text/plain", &bytes)
                .unwrap(),
            bytes: bytes.into(),
            state: AttachmentState::Failed(UploadError::Network),
            transfer: None,
            bytes_sent: false,
            retry_at: None,
        }
    }

    #[test]
    fn uploads_on_one_issue_and_bytes_held_in_all_are_bounded() {
        let mib = 1024 * 1024;
        // Eight uploads waiting on one issue: a ninth is refused.
        let eight: Vec<_> = (0..8).map(|_| upload(16)).collect();
        assert_eq!(admit(&eight, 128, 16), Err(UploadError::Budget));
        assert_eq!(admit(&eight[..7], 112, 16), Ok(()));
        // At most 32 MiB on one issue, failed uploads included.
        let held = [upload(9 * mib), upload(9 * mib), upload(9 * mib)];
        assert_eq!(admit(&held, 27 * mib, 6 * mib), Err(UploadError::Budget));
        assert_eq!(admit(&held, 27 * mib, 5 * mib), Ok(()));
        // At most 64 MiB held across drafts and uploads.
        assert_eq!(admit(&[], 60 * mib, 5 * mib), Err(UploadError::Budget));
    }
}
