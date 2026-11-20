//! Byte requests share the paced API workers and bounded admission.
use super::{
    Hub, HubError, ReadPriority, Shared, lock,
    worker::{Job, Task},
};
use crate::{
    Error, PageRequest,
    hub::AttachmentState,
    model::IssueMedia,
    uploads::{ImageDeclaration, UploadError},
};
use std::sync::Arc;
use tokio::sync::oneshot;
pub(super) const QUEUE_CAPACITY: usize = 100;

#[derive(Clone, Default)]
// qual:allow(srp, god_struct) reason: "One immutable snapshot exposes independently readable media metadata and load status."
pub struct FilesSnapshot {
    items: Vec<IssueMedia>,
    cursor: Option<String>,
    loading: bool,
    loaded: bool,
    failed: bool,
    cursors: std::collections::HashSet<String>,
    rescan: super::rescan::Rescan,
    /// Standalone attachments being added from the Attachments panel.
    uploads: Vec<AttachmentUpload>,
    /// Attachments being removed, by media ID.
    removing: std::collections::HashSet<String>,
    /// What happened to the last add or remove, when it needs saying.
    notice: Option<&'static str>,
}

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
    /// Whether the next step checks the transfer rather than sending.
    pub fn checks(&self) -> bool {
        self.bytes_sent
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

const CHANGED_ELSEWHERE: &str = "This attachment changed elsewhere. The list was read again.";

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

/// Standalone uploads run on a small pool, not a thread each: at most
/// [`WORKERS`] send at once, and the rest wait their turn, holding only the
/// bytes [`admit`] allowed.
mod uploads {
    use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};

    pub(super) const WORKERS: usize = 2;

    type Job = Box<dyn FnOnce() + Send>;

    pub(super) fn run(job: impl FnOnce() + Send + 'static) {
        static POOL: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();
        let pool = POOL.get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<Job>();
            let receiver = Arc::new(Mutex::new(receiver));
            for _ in 0..WORKERS {
                let receiver = Arc::clone(&receiver);
                let _ = std::thread::Builder::new()
                    .name("subspace-attachments".into())
                    .spawn(move || {
                        loop {
                            let job = receiver
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .recv();
                            let Ok(job) = job else {
                                return;
                            };
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                        }
                    });
            }
            Mutex::new(sender)
        });
        let _ = pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(Box::new(job));
    }
}

impl FilesSnapshot {
    /// Reads the list again from its first page, keeping what is shown
    /// until a complete scan says what is gone, and any upload in progress.
    pub(in crate::hub) fn reload(&mut self) {
        *self = Self {
            items: std::mem::take(&mut self.items),
            uploads: std::mem::take(&mut self.uploads),
            removing: std::mem::take(&mut self.removing),
            notice: self.notice.take(),
            ..Self::default()
        };
        self.rescan.begin();
    }
    /// Forgets the list and its read state, keeping uploads in progress.
    pub(in crate::hub) fn clear_keeping_uploads(&mut self) {
        *self = Self {
            uploads: std::mem::take(&mut self.uploads),
            ..Self::default()
        };
    }
    /// Standalone attachments being added.
    pub fn uploads(&self) -> &[AttachmentUpload] {
        &self.uploads
    }
    /// Whether this attachment is being removed.
    pub fn removing(&self, media: &str) -> bool {
        self.removing.contains(media)
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn notice(&self) -> Option<&'static str> {
        self.notice
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve immutable hub snapshots across the client boundary."
    pub fn items(&self) -> &[IssueMedia] {
        &self.items
    }
    pub fn is_idle(&self) -> bool {
        !self.loaded && !self.loading && !self.failed
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn is_failed(&self) -> bool {
        self.failed
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn is_loading(&self) -> bool {
        self.loading
    }
    pub fn has_more(&self) -> bool {
        self.cursor.is_some()
    }
}

/// Which read a byte request uses: image bytes, a text attachment or any
/// standalone file.
#[derive(Clone, Copy)]
pub(super) enum ByteRead {
    Image,
    Text,
    File,
}

pub(super) struct FileRead {
    pub media: IssueMedia,
    pub limit: usize,
    pub read: ByteRead,
    pub reply: std::sync::Mutex<Option<oneshot::Sender<Result<Vec<u8>, Error>>>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl FileRead {
    fn fetch(&self, client: &crate::Client) -> Result<Vec<u8>, Error> {
        match self.read {
            ByteRead::Image => client.fetch_image(&self.media, self.limit),
            ByteRead::Text => client.fetch_text(&self.media, self.limit),
            ByteRead::File => client.fetch_file(&self.media, self.limit),
        }
    }
}

impl Hub {
    pub async fn fetch_image(
        &self,
        generation: super::ScopeGeneration,
        revision: u64,
        media: IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        self.fetch_bytes((generation, revision), (media, limit), ByteRead::Image)
            .await
    }

    /// A text attachment's bytes, through the same paced, bounded byte reads
    /// as images.
    pub async fn fetch_text(
        &self,
        generation: super::ScopeGeneration,
        revision: u64,
        media: IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        self.fetch_bytes((generation, revision), (media, limit), ByteRead::Text)
            .await
    }

    /// A standalone attachment's bytes, of any type, for saving.
    pub async fn fetch_file(
        &self,
        generation: super::ScopeGeneration,
        revision: u64,
        media: IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        self.fetch_bytes((generation, revision), (media, limit), ByteRead::File)
            .await
    }

    async fn fetch_bytes(
        &self,
        (generation, revision): (super::ScopeGeneration, u64),
        (media, limit): (IssueMedia, usize),
        read: ByteRead,
    ) -> Result<Vec<u8>, Error> {
        let permit = self
            .0
            .file_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::Network)?;
        let (reply, receive) = oneshot::channel();
        {
            let mut state = lock(&self.0.state);
            if !state.issue_current(generation, revision) {
                return Err(Error::InvalidRequest);
            }
            self.0.enqueue(
                &mut state,
                Task::MediaBytes(FileRead {
                    media,
                    limit,
                    read,
                    reply: std::sync::Mutex::new(Some(reply)),
                    _permit: permit,
                }),
                ReadPriority::Visible,
            );
        }
        receive.await.unwrap_or(Err(Error::Network))
    }

    // qual:allow(dry, fragment) reason: "The UI boundary only locks state and queues a navigation-fenced read."
    pub fn load_attachments(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state.snapshot.issue.as_mut().filter(|i| i.detail.is_some()) else {
            return Ok(());
        };
        let files = &mut issue.files;
        if files.loading || (files.loaded && !files.has_more() && !files.failed) {
            return Ok(());
        }
        files.loading = true;
        files.failed = false;
        let task = Task::Files(issue.identifier.clone(), files.cursor.clone());
        state.changed();
        self.0.enqueue(&mut state, task, ReadPriority::Visible);
        Ok(())
    }
}

pub(super) fn bytes(shared: &Shared, job: &Job, request: &FileRead) {
    if lock(&request.reply)
        .as_ref()
        .is_none_or(|reply| reply.is_closed())
    {
        return;
    }
    let sent = std::time::Instant::now();
    let result = request.fetch(&shared.client);
    // A NETT token refused before it expires is renewed rather than shown
    // as signed out; the byte read itself still fails.
    let refused = match result {
        Err(error @ (Error::SignInNeeded | Error::Forbidden)) => {
            super::renewal::stands(shared, job.generation, sent, error)
        }
        _ => false,
    };
    let mut state = lock(&shared.state);
    let result = if !job.current(&state) {
        Err(Error::InvalidRequest)
    } else {
        result
    };
    if refused && matches!(result, Err(Error::SignInNeeded | Error::Forbidden)) {
        state.snapshot.status = super::HubStatus::Error(
            result
                .as_ref()
                .err()
                .copied()
                .unwrap_or(Error::Forbidden)
                .into(),
        );
        state.changed();
    }
    if let Some(reply) = lock(&request.reply).take() {
        _ = reply.send(result);
    }
}

pub(super) fn load(shared: &Shared, job: &Job, issue: &str, cursor: Option<&str>) {
    let sent = std::time::Instant::now();
    let result = shared.client.attachments(
        issue,
        &PageRequest {
            cursor: cursor.map(str::to_owned),
            ..PageRequest::default()
        },
    );
    let refused = match result {
        Err(error @ (Error::SignInNeeded | Error::Forbidden)) => {
            super::renewal::stands(shared, job.generation, sent, error)
        }
        _ => false,
    };
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    let Some(issue) = state.snapshot.issue.as_mut() else {
        return;
    };
    let files = &mut issue.files;
    files.loading = false;
    match result {
        Ok(page) => {
            let last = page.next_cursor.is_none();
            files
                .rescan
                .absorb(&mut files.items, page.items, last, |item| &item.media_id);
            files.failed = page
                .next_cursor
                .as_ref()
                .is_some_and(|next| !files.cursors.insert(next.clone()));
            files.cursor = page.next_cursor;
            files.loaded = true;
        }
        Err(error) => {
            files.failed = true;
            files.rescan.abandon();
            if refused {
                state.snapshot.status = super::HubStatus::Error(error.into());
                state.changed();
                return;
            }
        }
    }
    if files.has_more() && !files.failed {
        files.loading = true;
        let task = Task::Files(issue.identifier.clone(), files.cursor.clone());
        shared.enqueue(&mut state, task, ReadPriority::LaterPage);
    }
    state.changed();
}

/// The issue, its opening and the scope a standalone attachment is added
/// to, taken when the person starts choosing a file. Nothing is added if
/// any of them changed, even if the same issue was opened again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentTarget {
    generation: super::ScopeGeneration,
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
        let upload = AttachmentUpload {
            id: super::comments::next_attachment_id(),
            request_key: uuid::Uuid::new_v4().to_string(),
            declaration,
            bytes,
            state: AttachmentState::Uploading,
            transfer: None,
            bytes_sent: false,
            retry_at: None,
        };
        issue.files.notice = None;
        issue.files.uploads.push(upload.clone());
        let identifier = issue.identifier.clone();
        let generation = target.generation;
        state.attachment_buffers.push(buffer);
        state.changed();
        let shared = Arc::clone(&self.0);
        uploads::run(move || send(&shared, generation, &identifier, &upload));
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
        upload.state = if upload.bytes_sent {
            AttachmentState::Checking
        } else {
            AttachmentState::Uploading
        };
        let upload = upload.clone();
        let identifier = issue.identifier.clone();
        state.changed();
        let shared = Arc::clone(&self.0);
        uploads::run(move || send(&shared, generation, &identifier, &upload));
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
            uploads::run(move || {
                shared
                    .client
                    .cancel_issue_attachment(&identifier, &transfer)
            });
        }
    }

    /// Removes a standalone attachment at the version listed. A version
    /// that changed elsewhere reads the list again instead.
    pub fn remove_attachment(&self, media_id: &str) {
        let mut state = lock(&self.0.state);
        let generation = state.snapshot.generation;
        let Some(issue) = state.snapshot.issue.as_mut() else {
            return;
        };
        let Some(media) = issue
            .files
            .items
            .iter()
            .find(|m| m.media_id == media_id)
            .cloned()
        else {
            return;
        };
        if !issue.files.removing.insert(media.media_id.clone()) {
            return;
        }
        issue.files.notice = None;
        let identifier = issue.identifier.clone();
        state.changed();
        let shared = Arc::clone(&self.0);
        std::thread::spawn(move || {
            let result = shared.client.delete_media(&media);
            let mut state = lock(&shared.state);
            if !state.current(generation) {
                return;
            }
            let Some(issue) = state
                .snapshot
                .issue
                .as_mut()
                .filter(|issue| issue.identifier == identifier)
            else {
                return;
            };
            issue.files.removing.remove(&media.media_id);
            let reread = match result {
                Ok(()) => {
                    issue.files.items.retain(|m| m.media_id != media.media_id);
                    false
                }
                Err(UploadError::Refused { status: 409, .. }) => {
                    issue.files.notice = Some(CHANGED_ELSEWHERE);
                    true
                }
                Err(_) => {
                    issue.files.notice = Some("Couldn't remove the attachment. Try again.");
                    false
                }
            };
            if reread {
                reload_files(&shared, &mut state);
            }
            state.changed();
        });
    }
}

impl super::State {
    /// Keeps the open issue's uploads aside as it closes or another issue
    /// opens in its place: a transfer may finish, or need a check, later.
    pub(in crate::hub) fn park_open_uploads(&mut self) {
        let Some(issue) = self.snapshot.issue.as_mut() else {
            return;
        };
        let uploads = std::mem::take(&mut issue.files.uploads);
        if !uploads.is_empty() {
            self.parked_uploads
                .entry(issue.identifier.clone())
                .or_default()
                .extend(uploads);
        }
    }

    /// Shows the uploads kept for `issue` again as it opens.
    pub(in crate::hub) fn unpark_uploads(&mut self, issue: &mut super::IssueSnapshot) {
        if let Some(uploads) = self.parked_uploads.remove(&issue.identifier) {
            issue.files.uploads.extend(uploads);
        }
    }

    /// The upload `id` of `issue`, whether the issue is open or its uploads
    /// were kept aside when it closed.
    fn upload_row(&mut self, issue: &str, id: &str) -> Option<&mut AttachmentUpload> {
        let open = self
            .snapshot
            .issue
            .as_mut()
            .filter(|open| open.identifier == issue)
            .map(|open| &mut open.files.uploads);
        let uploads = match open {
            Some(uploads) => uploads,
            None => self.parked_uploads.get_mut(issue)?,
        };
        uploads.iter_mut().find(|upload| upload.id == id)
    }

    /// Forgets the upload `id` of `issue` once it is stored.
    fn remove_upload(&mut self, issue: &str, id: &str) {
        if let Some(open) = self
            .snapshot
            .issue
            .as_mut()
            .filter(|open| open.identifier == issue)
        {
            open.files.uploads.retain(|upload| upload.id != id);
        }
        if let Some(parked) = self.parked_uploads.get_mut(issue) {
            parked.retain(|upload| upload.id != id);
            if parked.is_empty() {
                self.parked_uploads.remove(issue);
            }
        }
    }
}

/// Reads the open issue's attachment list again from its first page.
fn reload_files(shared: &Shared, state: &mut super::State) {
    let Some(issue) = state.snapshot.issue.as_mut() else {
        return;
    };
    issue.files.reload();
    issue.files.loading = true;
    let task = Task::Files(issue.identifier.clone(), None);
    shared.enqueue(state, task, ReadPriority::Visible);
}

fn send(
    shared: &Shared,
    generation: super::ScopeGeneration,
    identifier: &str,
    upload: &AttachmentUpload,
) {
    // Once the bytes may be stored, only the transfer is completed.
    let (result, transfer, bytes_sent) = match (&upload.transfer, upload.bytes_sent) {
        (Some(transfer), true) => (
            shared
                .client
                .complete_issue_attachment(identifier, transfer, &upload.declaration),
            Some(transfer.clone()),
            true,
        ),
        _ => {
            let attempt = shared.client.upload_issue_attachment(
                identifier,
                &upload.request_key,
                &upload.declaration,
                &upload.bytes,
            );
            (
                attempt.result.map(Some),
                attempt.transfer.or_else(|| upload.transfer.clone()),
                attempt.bytes_sent,
            )
        }
    };
    let mut state = lock(&shared.state);
    if !state.current(generation) {
        return;
    }
    // The issue may have closed meanwhile: its uploads were kept aside, and
    // the outcome is kept with them for when it reopens.
    let open = state
        .snapshot
        .issue
        .as_ref()
        .is_some_and(|issue| issue.identifier == identifier);
    let Some(row) = state.upload_row(identifier, &upload.id) else {
        return;
    };
    if settle(row, result, transfer, bytes_sent) {
        state.remove_upload(identifier, &upload.id);
        if open {
            reload_files(shared, &mut state);
        }
    }
    state.changed();
}

/// Records what an attempt did to `row`. True when the file was stored.
fn settle(
    row: &mut AttachmentUpload,
    result: Result<Option<IssueMedia>, UploadError>,
    transfer: Option<String>,
    bytes_sent: bool,
) -> bool {
    row.transfer = transfer;
    row.bytes_sent = row.bytes_sent || bytes_sent;
    match result {
        Ok(Some(_)) => true,
        // Refused for good: nothing was stored, so a retry starts afresh
        // under a new key.
        Ok(None) => {
            row.state = AttachmentState::NotFound;
            row.transfer = None;
            row.bytes_sent = false;
            row.request_key = uuid::Uuid::new_v4().to_string();
            false
        }
        Err(error) => {
            let wait = match error {
                UploadError::Unclear { retry_after } | UploadError::Refused { retry_after, .. } => {
                    retry_after
                }
                _ => 0,
            };
            // A wait past what the clock can represent still holds.
            row.retry_at = (wait > 0).then(|| {
                crate::nett::flow::deadline(
                    std::time::Instant::now(),
                    std::time::Duration::from_secs(wait),
                )
            });
            if matches!(error, UploadError::Refused { status: 409, .. }) && !row.bytes_sent {
                // The reservation can no longer be used (it expired or was
                // cancelled) and no bytes went: the next try reserves anew.
                row.transfer = None;
                row.request_key = uuid::Uuid::new_v4().to_string();
            }
            row.state = AttachmentState::Failed(error);
            false
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

#[cfg(test)]
mod parking_tests {
    use crate::hub::{AttachmentState, Hub, IssueSnapshot, IssueStatus, lock};
    use crate::uploads::UploadError;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use subspace_framework::client_test_support::{Mock, Request};

    const UPLOAD: &str = "upload-parked-on-close";

    /// NETT serving attachments: its capability list, the reservation and
    /// the file host. The test answers completion itself.
    fn nett(base: &Arc<Mutex<String>>, request: &Request) -> (u16, String) {
        let path = request.path.as_str();
        if path.ends_with("/desktop/capabilities") {
            (200, r#"{"features":["attachments"]}"#.into())
        } else if path.ends_with("/attachments/prepare") {
            let url = format!("{}/files/{UPLOAD}", lock(base));
            let reply = serde_json::json!({"uploadId": UPLOAD, "uploadUrl": url, "method": "PUT"});
            (200, reply.to_string())
        } else if request.method == "PUT" && path.ends_with(UPLOAD) {
            (200, String::new())
        } else {
            (404, "{}".into())
        }
    }

    fn wait_for(what: &str, done: &dyn Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn an_upload_in_progress_when_its_issue_closes_is_shown_again_when_it_reopens() {
        let base = Arc::new(Mutex::new(String::new()));
        let served = Arc::clone(&base);
        let mock = Mock::with_headers(move |request| {
            if request.path.ends_with("/attachments/from-upload") {
                std::thread::sleep(Duration::from_millis(400));
                return (202, vec![("Retry-After".into(), "120".into())], "{}".into());
            }
            let (status, body) = nett(&served, request);
            (status, Vec::new(), body)
        });
        *lock(&base) = mock.base_url().to_owned();
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-0000000000f1"),
            Some(subspace_framework::core::Secret::new("nett-parked-upload")),
        )
        .unwrap()
        .without_live();
        let hub = Hub::from_client(client, "NETT");
        {
            // The issue is open with its detail, as once it has loaded.
            let mut state = lock(&hub.0.state);
            let mut issue = IssueSnapshot::new("OPS-1", crate::PROFILE_ID, IssueStatus::Loaded);
            issue.detail = Some(serde_json::from_value(crate::tests::detail("1")).unwrap());
            state.snapshot.issue = Some(issue);
        }
        hub.add_attachment(
            "parked.txt",
            "text/plain",
            b"Checked after the issue closed.\n".to_vec(),
        )
        .unwrap();
        // The bytes reached the file host and completion is under way when
        // the person moves away from the issue.
        wait_for("the completion", &|| {
            mock.requests()
                .iter()
                .any(|request| request.path.ends_with("/attachments/from-upload"))
        });
        hub.close_issue();
        wait_for("the outcome, kept aside", &|| {
            lock(&hub.0.state)
                .parked_uploads
                .get("OPS-1")
                .is_some_and(|uploads| matches!(uploads[0].state, AttachmentState::Failed(_)))
        });
        hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
        let snapshot = hub.snapshot();
        let uploads = snapshot.issue().unwrap().files().uploads();
        assert_eq!(uploads.len(), 1, "the upload is shown again");
        assert!(uploads[0].checks(), "Check completes the same transfer");
        assert!(matches!(
            uploads[0].state(),
            AttachmentState::Failed(UploadError::Unclear { retry_after: 120 })
        ));
        assert!(lock(&hub.0.state).parked_uploads.is_empty());
        hub.stop();
    }

    #[test]
    fn uploads_kept_aside_are_never_shown_to_another_account() {
        let mut state = crate::hub::State::new("http://127.0.0.1:1/api", "NETT");
        state
            .parked_uploads
            .insert("OPS-1".into(), vec![super::admission_tests::upload(8)]);
        state.snapshot.me = Some(crate::model::Me {
            user_id: "first".into(),
            kind: "human".into(),
            display_name: "Fixture".into(),
        });
        assert!(state.retire_if_account_changed(&Ok("second".into()), None));
        assert!(state.parked_uploads.is_empty());
    }
}
