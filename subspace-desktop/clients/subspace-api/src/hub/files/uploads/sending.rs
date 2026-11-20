//! Sending an upload, or completing its transfer, on a small pool, and
//! recording what the attempt did.
use super::{AttachmentUpload, Cancel, UploadIssue};
use crate::hub::{AttachmentState, Shared, lock};
use crate::{model::IssueMedia, uploads::UploadError};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};

/// Uploads sent at once.
const WORKERS: usize = 2;

type Job = Box<dyn FnOnce() + Send>;

/// Runs `job` on the upload pool. Standalone uploads run on a small pool,
/// not a thread each: at most [`WORKERS`] send at once, and the rest wait
/// their turn, holding only the bytes admission allowed: on their issue
/// ([`admit_here`](super::admit_here)), then in the shared budget
/// ([`UploadStore::room`](super::UploadStore::room)).
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

/// What one attempt did: its outcome, the transfer it holds and whether
/// its bytes may have gone.
type Outcome = (
    Result<Option<IssueMedia>, UploadError>,
    Option<String>,
    bool,
);

/// The issue an attempt is for. Every request names it by its stable ID,
/// which still names it after a move; a reservation it leaves behind is
/// cancelled under it, and its outcome matched to the open issue by it.
pub(super) struct Attempt {
    issue: UploadIssue,
}

impl Attempt {
    pub(super) fn new(issue: UploadIssue) -> Self {
        Self { issue }
    }
}

/// Whether a job starts its attempt.
enum Start {
    /// Its upload still waits for it.
    Run,
    /// Its upload was let go while it waited for the pool: the reservation
    /// it holds is no one's.
    LetGo,
    /// The account or hub it was for is gone.
    Gone,
}

/// Sends `upload`, or completes its transfer once its bytes may be stored,
/// and records the outcome on its row, wherever its issue's uploads are. A
/// job whose upload was let go while it waited for the pool sends nothing
/// and cancels the reservation it holds.
pub(super) fn send(
    shared: &Shared,
    generation: crate::hub::ScopeGeneration,
    attempt: &Attempt,
    upload: &AttachmentUpload,
) {
    match start(shared, generation, &upload.id) {
        Start::Run => {}
        Start::LetGo => {
            let cancel = upload
                .transfer
                .clone()
                .map(|transfer| (attempt.issue.id.clone(), transfer));
            shared.cancel_reservations(cancel.into_iter().collect());
            return;
        }
        Start::Gone => return,
    }
    let outcome = transfer(shared, &attempt.issue.id, upload);
    let mut state = lock(&shared.state);
    if !state.current(generation) {
        return;
    }
    let cancels = record(shared, &mut state, (attempt, upload), outcome);
    state.changed();
    shared.cancel_reservations(cancels);
}

/// Whether the job for the upload `id` starts, in this scope.
fn start(shared: &Shared, generation: crate::hub::ScopeGeneration, id: &str) -> Start {
    let state = lock(&shared.state);
    if !state.current(generation) {
        Start::Gone
    } else if state.uploads.contains(id) {
        Start::Run
    } else {
        Start::LetGo
    }
}

/// Sends `upload` to the issue `reference`, its ID, names, or, once its
/// bytes may be stored, only completes its transfer.
fn transfer(shared: &Shared, reference: &str, upload: &AttachmentUpload) -> Outcome {
    match upload.transfer.as_ref().filter(|_| upload.checks()) {
        Some(transfer) => (
            shared
                .client
                .complete_issue_attachment(reference, transfer, &upload.declaration),
            Some(transfer.clone()),
            true,
        ),
        _ => {
            let attempt = shared.client.upload_issue_attachment(
                reference,
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
    }
}

/// Records `outcome` on the upload's row, and reads the open issue's
/// attachments again once the file is stored, row or not. An upload let go
/// while its attempt was under way has no row: a reservation it left
/// behind is no one's, so it is cancelled under the issue's ID. The store
/// then follows the open issue, which lets go of a closed issue's upload
/// that failed with nothing left to check. Returns the reservations to
/// cancel.
fn record(
    shared: &Shared,
    state: &mut crate::hub::State,
    (attempt, upload): (&Attempt, &AttachmentUpload),
    (result, transfer, bytes_sent): Outcome,
) -> Vec<Cancel> {
    let stored = matches!(result, Ok(Some(_)));
    let mut cancels: Vec<Cancel> = match state.uploads.row(&upload.id) {
        Some(row) => {
            settle(row, result, transfer, bytes_sent);
            Vec::new()
        }
        None => transfer
            .filter(|_| !stored)
            .map(|transfer| (attempt.issue.id.clone(), transfer))
            .into_iter()
            .collect(),
    };
    if stored {
        state.uploads.remove(&upload.id);
        if state.open_names(&attempt.issue) {
            crate::hub::files::reload_files(shared, state);
        }
    }
    cancels.extend(state.follow_uploads(std::time::Instant::now()));
    cancels
}

/// Records what an attempt did to `row`. A stored file's row is taken out
/// by the caller.
fn settle(
    row: &mut AttachmentUpload,
    result: Result<Option<IssueMedia>, UploadError>,
    transfer: Option<String>,
    bytes_sent: bool,
) {
    row.transfer = transfer;
    row.bytes_sent = row.bytes_sent || bytes_sent;
    match result {
        Ok(Some(_)) => {}
        // Refused for good: nothing was stored, so a retry starts afresh
        // under a new key.
        Ok(None) => {
            row.state = AttachmentState::NotFound;
            row.transfer = None;
            row.bytes_sent = false;
            row.request_key = uuid::Uuid::new_v4().to_string();
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
        }
    }
}

#[cfg(test)]
mod sending_tests;
