//! Sending an upload, or completing its transfer, on a small pool, and
//! recording what the attempt did.
use super::{AttachmentUpload, Cancel};
use crate::hub::{AttachmentState, Shared, lock};
use crate::{model::IssueMedia, uploads::UploadError};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};

/// Uploads sent at once.
const WORKERS: usize = 2;

type Job = Box<dyn FnOnce() + Send>;

/// Runs `job` on the upload pool. Standalone uploads run on a small pool,
/// not a thread each: at most [`WORKERS`] send at once, and the rest wait
/// their turn, holding only the bytes [`admit`](super::admit) allowed.
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

/// Sends `upload`, or completes its transfer once its bytes may be stored,
/// and records the outcome on its row, wherever its issue's uploads are.
pub(super) fn send(
    shared: &Arc<Shared>,
    generation: crate::hub::ScopeGeneration,
    identifier: &str,
    upload: &AttachmentUpload,
) {
    let outcome = attempt(shared, identifier, upload);
    let mut state = lock(&shared.state);
    if !state.current(generation) {
        return;
    }
    let cancels = record(shared, &mut state, (identifier, upload), outcome);
    state.changed();
    shared.cancel_reservations(cancels);
}

/// Sends `upload`, or, once its bytes may be stored, only completes its
/// transfer.
fn attempt(shared: &Shared, identifier: &str, upload: &AttachmentUpload) -> Outcome {
    match upload.transfer.as_ref().filter(|_| upload.checks()) {
        Some(transfer) => (
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
    }
}

/// Records `outcome` on the upload's row. The issue may have closed
/// meanwhile: its uploads were kept aside, and the outcome is kept with
/// them for when it reopens, unless it failed with nothing left to check.
/// Returns the reservations to cancel: that one's, or one made by an upload
/// let go while its attempt was under way, which is no one's now.
fn record(
    shared: &Shared,
    state: &mut crate::hub::State,
    (identifier, upload): (&str, &AttachmentUpload),
    (result, transfer, bytes_sent): Outcome,
) -> Vec<Cancel> {
    let open = state.upload_open(&upload.id);
    let Some(row) = state.upload_row(&upload.id) else {
        let stored = matches!(result, Ok(Some(_)));
        return transfer
            .filter(|_| !stored)
            .map(|transfer| (identifier.to_owned(), transfer))
            .into_iter()
            .collect();
    };
    let stored = settle(row, result, transfer, bytes_sent);
    let kept = open || row.worth_keeping();
    if stored {
        state.remove_upload(&upload.id);
        if open {
            crate::hub::files::reload_files(shared, state);
        }
    } else if !kept {
        let gone = state.remove_upload(&upload.id);
        return gone
            .and_then(|gone| gone.cancel_on(identifier))
            .into_iter()
            .collect();
    }
    Vec::new()
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
