//! Sending an upload, or completing its transfer, on a small pool, and
//! recording what the attempt did.
use super::AttachmentUpload;
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

/// Sends `upload`, or completes its transfer once its bytes may be stored,
/// and records the outcome on its row, wherever its issue's uploads are.
pub(super) fn send(
    shared: &Shared,
    generation: crate::hub::ScopeGeneration,
    identifier: &str,
    upload: &AttachmentUpload,
) {
    // Once the bytes may be stored, only the transfer is completed.
    let (result, transfer, bytes_sent) = match upload.transfer.as_ref().filter(|_| upload.checks())
    {
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
            crate::hub::files::reload_files(shared, &mut state);
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
