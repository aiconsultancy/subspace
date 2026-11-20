use super::recovery::{check, cleanup, scan};
use super::{Attachment, AttachmentState, DraftKey};
use crate::{
    hub::{ScopeGeneration, Shared, lock},
    uploads::UploadError,
};
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

pub(super) fn start(
    shared: Arc<Shared>,
    generation: ScopeGeneration,
    key: DraftKey,
    image: Attachment,
) {
    if image.running.swap(true, Ordering::AcqRel) {
        return;
    }
    std::thread::spawn(move || {
        let _serial = lock(&shared.upload_lock);
        let result = run(&shared, generation, &key, &image);
        finish(&shared, &key, &image, result);
        wake_retry(&shared, &image);
    });
}

fn run(
    shared: &Shared,
    generation: ScopeGeneration,
    key: &DraftKey,
    image: &Attachment,
) -> Result<(), UploadError> {
    if lock(&image.inner).removed {
        return cleanup(&shared.client, key, image);
    }
    let current = lock(&shared.state).current(generation);
    if !current {
        return Err(UploadError::Removed);
    }
    if lock(&image.inner).uncertain {
        check(&shared.client, key, image)?;
        if lock(&image.inner).removed {
            cleanup(&shared.client, key, image)?;
        }
        return Ok(());
    }
    // NETT finds an uncertain upload again by its request key; Subspace API
    // by what is new on the issue since this upload began.
    let nett = shared.client.service() == crate::Backend::Nett;
    let before = if nett {
        Vec::new()
    } else {
        scan(&shared.client, &key.issue).map_err(|_| UploadError::Network)?
    };
    if !lock(&shared.state).current(generation) {
        return Err(UploadError::Removed);
    }
    {
        let mut row = lock(&image.inner);
        row.existing = Some(before.into_iter().map(|media| media.media_id).collect());
        if row.removed {
            return Ok(());
        }
        if nett {
            row.request_key = Some(uuid::Uuid::new_v4().to_string());
        }
        row.state = AttachmentState::Uploading;
    }
    lock(&shared.state).changed();
    send(shared, key, image)?;
    if lock(&image.inner).removed {
        cleanup(&shared.client, key, image)?;
    }
    Ok(())
}

fn send(shared: &Shared, key: &DraftKey, image: &Attachment) -> Result<(), UploadError> {
    let (declaration, bytes, draft, request_key) = {
        let row = lock(&image.inner);
        (
            row.declaration.clone(),
            row.bytes.clone(),
            row.media_draft.clone(),
            row.request_key.clone().unwrap_or_default(),
        )
    };
    let result = shared.client.upload_comment_media(
        &key.issue,
        (&draft, &request_key),
        &declaration,
        &bytes,
    );
    {
        let mut row = lock(&image.inner);
        match result {
            Ok(media) => {
                row.media = Some(media);
                row.state = AttachmentState::Ready;
            }
            Err(error) => {
                row.uncertain =
                    matches!(error, UploadError::Checking | UploadError::Unclear { .. });
                return Err(error);
            }
        }
    }
    Ok(())
}

fn finish(shared: &Shared, key: &DraftKey, image: &Attachment, result: Result<(), UploadError>) {
    let mut state = lock(&shared.state);
    let mut row = lock(&image.inner);
    let done = result.is_ok() && row.removed;
    if let Err(error) = result {
        row.state = AttachmentState::Failed(error);
        if let UploadError::Refused { retry_after, .. } | UploadError::Unclear { retry_after } =
            error
        {
            row.retry_at = Instant::now().checked_add(Duration::from_secs(retry_after));
        }
    }
    if done || row.state == AttachmentState::Ready {
        row.bytes = Arc::from([]);
    }
    let id = row.id.clone();
    let orphan = row.checked_absent;
    drop(row);
    image.running.store(false, Ordering::Release);
    if done && let Some(draft) = state.drafts.get_mut(key) {
        draft.images.retain(|image| image.id() != id);
        if orphan {
            draft.notice =
                Some("Attachment removed. An orphaned upload may still exist on the server.");
        }
    }
    state.changed();
}

fn wake_retry(shared: &Arc<Shared>, image: &Attachment) {
    let deadline = lock(&image.inner).retry_at;
    let Some(delay) = deadline.and_then(|at| at.checked_duration_since(Instant::now())) else {
        return;
    };
    let shared = Arc::downgrade(shared);
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if let Some(shared) = shared.upgrade() {
            let mut state = lock(&shared.state);
            if state.is_running() {
                state.changed();
            }
        }
    });
}
