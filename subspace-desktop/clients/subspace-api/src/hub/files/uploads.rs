//! Standalone attachments being added: admitted within the shared byte
//! budget, sent on a small pool, kept in one store under their issue's ID
//! while it is closed, and, once their bytes may be stored, completed
//! rather than sent again.
mod again;
mod sending;
mod store;

pub(in crate::hub) use store::{Cancel, UploadIssue, UploadStore};

use crate::{
    hub::{AttachmentState, Hub, Shared, State, lock},
    uploads::{ImageDeclaration, UploadError},
};
use std::sync::Arc;
use std::time::Instant;

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
    /// Whether its bytes were let go, kept for a closed issue too long or
    /// to make room: a check needs none, but sending again needs the file
    /// chosen again.
    released: bool,
    /// Checks in a row that could not tell whether its file was stored.
    unknown_checks: u8,
}

/// Checks in a row that could not tell whether a file was stored before
/// the person may choose to send it again.
const MAX_UNKNOWN_CHECKS: u8 = 3;

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
            released: false,
            unknown_checks: 0,
        }
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn file_name(&self) -> &str {
        &self.declaration.file_name
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
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
    /// offered, as it is for one that would be sent again after its bytes
    /// were let go.
    pub fn retryable(&self) -> bool {
        if self.released && !self.checks() {
            return false;
        }
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
    /// Whether its bytes were let go: sending it again needs the file
    /// chosen again.
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn released(&self) -> bool {
        self.released
    }
    /// Whether the person may choose to send the file again, warned that it
    /// may then be attached twice: checks in a row could not tell whether
    /// it was stored, and its bytes are still held.
    pub fn may_send_again(&self) -> bool {
        matches!(self.state, AttachmentState::Failed(_))
            && self.checks()
            && !self.released
            && self.unknown_checks >= MAX_UNKNOWN_CHECKS
    }
}

#[cfg(test)]
impl AttachmentUpload {
    /// A failed upload of `size` bytes whose bytes may be stored under
    /// `transfer`, waiting for a check.
    pub(in crate::hub) fn awaiting_check_for_test(size: usize, transfer: &str) -> Self {
        let mut upload = admission_tests::upload(8);
        upload.bytes = vec![b'a'; size].into();
        upload.id = crate::hub::comments::next_attachment_id();
        upload.state = AttachmentState::Failed(UploadError::Unclear { retry_after: 0 });
        upload.transfer = Some(transfer.to_owned());
        upload.bytes_sent = true;
        upload
    }

    /// Its bytes, as the shared budget counts them.
    pub(in crate::hub) fn buffer_for_test(&self) -> std::sync::Weak<[u8]> {
        Arc::downgrade(&self.bytes)
    }
}

impl std::fmt::Debug for AttachmentUpload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AttachmentUpload([redacted])")
    }
}

/// Whether one more file of `size` bytes may wait on an issue beside
/// `uploads`: at most eight uploads and 32 MiB, as for a comment draft. A
/// failed upload keeps its bytes for Retry, so it counts until it is stored
/// or dismissed. The budget across the app is the store's to decide, after
/// this.
fn admit_here<'a>(
    uploads: impl Iterator<Item = &'a AttachmentUpload>,
    size: usize,
) -> Result<(), UploadError> {
    let (count, here) = uploads.fold((0, 0usize), |(count, here), upload| {
        (count + 1, here.saturating_add(upload.bytes.len()))
    });
    if count >= crate::uploads::MAX_DRAFT_IMAGES
        || here.saturating_add(size) > crate::uploads::MAX_DRAFT_BYTES
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
    /// issue or the scope is no longer the one the file was chosen for. The
    /// file is admitted on its issue first; only then does the shared budget
    /// decide, letting go of uploads kept out of sight only if that lets it
    /// in.
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
        self.0.follow_uploads(&mut state);
        let issue = target_issue(&state, target).ok_or(UploadError::Removed)?;
        let open = state.open_for_uploads();
        admit_here(state.uploads.shown(open.as_ref()), bytes.len())?;
        let held = state.held_bytes();
        state.uploads.room(held, bytes.len())?;
        let bytes: Arc<[u8]> = bytes.into();
        state.attachment_buffers.push(Arc::downgrade(&bytes));
        let upload = AttachmentUpload::new(declaration, bytes);
        state
            .uploads
            .add(upload.clone(), issue.clone(), target.opening);
        if let Some(open) = state.snapshot.issue.as_mut() {
            open.files.notice = None;
        }
        state.changed();
        let attempt = sending::Attempt::new(issue);
        let (shared, generation) = (Arc::clone(&self.0), target.generation);
        sending::run(move || sending::send(&shared, generation, &attempt, &upload));
        Ok(())
    }

    /// Checks an upload whose bytes may have been stored by completing its
    /// transfer, or sends one whose bytes were not, under the same key. A
    /// wait NETT asked for is honoured: nothing is sent before it ends.
    pub fn retry_attachment_upload(&self, id: &str) {
        let mut state = lock(&self.0.state);
        self.0.follow_uploads(&mut state);
        let generation = state.snapshot.generation;
        let Some(retry) = retry_row(&mut state, id) else {
            return;
        };
        state.changed();
        let Retry::Send(send) = retry else {
            return;
        };
        let (attempt, upload) = *send;
        let shared = Arc::clone(&self.0);
        sending::run(move || sending::send(&shared, generation, &attempt, &upload));
    }

    /// Stops showing an upload that failed, cancelling its reservation when
    /// it was never completed.
    pub fn dismiss_attachment_upload(&self, id: &str) {
        let mut state = lock(&self.0.state);
        self.0.follow_uploads(&mut state);
        let open = state.open_for_uploads();
        let failed = state
            .uploads
            .shown_row(open.as_ref(), id)
            .is_some_and(|(upload, _)| !upload.in_flight());
        if !failed {
            return;
        }
        let cancel = state
            .uploads
            .remove(id)
            .and_then(|(upload, issue)| upload.cancel_on(&issue.id));
        state.changed();
        self.0.cancel_reservations(cancel.into_iter().collect());
    }
}

/// The issue `target` names, if it is still the open one, in the same
/// opening, with its detail read.
fn target_issue(state: &State, target: &AttachmentTarget) -> Option<UploadIssue> {
    let issue = state
        .snapshot
        .issue
        .as_ref()
        .filter(|issue| issue.identifier == target.issue && issue.opening == target.opening)?;
    Some(UploadIssue {
        id: issue.detail.as_ref()?.fields.id.clone(),
        identifier: issue.identifier.clone(),
    })
}

/// What Retry does with an open issue's upload.
enum Retry {
    /// NETT asked to wait: the person is told so.
    Wait,
    /// The attempt to send.
    Send(Box<(sending::Attempt, AttachmentUpload)>),
}

/// Marks the open issue's upload `id` for another attempt, or the issue's
/// panel with why it must wait. `None` when there is nothing to retry.
fn retry_row(state: &mut State, id: &str) -> Option<Retry> {
    let open = state.open_for_uploads();
    let (upload, issue) = state.uploads.shown_row(open.as_ref(), id)?;
    if !upload.retryable() {
        return None;
    }
    if upload.retry_delay().is_some() {
        if let Some(open) = state.snapshot.issue.as_mut() {
            open.files.notice = Some("NETT asked to wait before this upload is checked again.");
        }
        return Some(Retry::Wait);
    }
    upload.state = if upload.checks() {
        AttachmentState::Checking
    } else {
        AttachmentState::Uploading
    };
    Some(Retry::Send(Box::new((
        sending::Attempt::new(issue.clone()),
        upload.clone(),
    ))))
}

impl Shared {
    /// Cancels the reservations of uploads let go, best effort, on the
    /// upload pool.
    pub(in crate::hub) fn cancel_reservations(&self, cancels: Vec<Cancel>) {
        cancels.into_iter().for_each(|(issue, transfer)| {
            let client = self.client.clone();
            sending::run(move || client.cancel_issue_attachment(&issue, &transfer));
        });
    }

    /// Follows the open issue in the upload store, as every use of it does,
    /// cancelling the reservations of what it lets go.
    pub(in crate::hub) fn follow_uploads(&self, state: &mut State) {
        let cancels = state.follow_uploads(Instant::now());
        self.cancel_reservations(cancels);
    }
}

#[cfg(test)]
mod admission_tests {
    use super::{AttachmentUpload, admit_here};
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
            released: false,
            unknown_checks: 0,
        }
    }

    #[test]
    fn uploads_waiting_on_one_issue_are_bounded() {
        let mib = 1024 * 1024;
        // Eight uploads waiting on one issue: a ninth is refused.
        let eight: Vec<_> = (0..8).map(|_| upload(16)).collect();
        assert_eq!(admit_here(eight.iter(), 16), Err(UploadError::Budget));
        assert_eq!(admit_here(eight[..7].iter(), 16), Ok(()));
        // At most 32 MiB on one issue, failed uploads included.
        let held = [upload(9 * mib), upload(9 * mib), upload(9 * mib)];
        assert_eq!(admit_here(held.iter(), 6 * mib), Err(UploadError::Budget));
        assert_eq!(admit_here(held.iter(), 5 * mib), Ok(()));
    }
}
