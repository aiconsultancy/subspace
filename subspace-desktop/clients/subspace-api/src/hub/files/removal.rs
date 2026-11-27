//! Removing a standalone attachment at the version listed.
use crate::hub::{Hub, lock};
use crate::uploads::{AttachmentProtocol, UploadError};
use std::sync::Arc;

impl Hub {
    /// Removes a standalone attachment at the version listed. A version
    /// that changed elsewhere reads the list again instead.
    pub fn remove_attachment(&self, media_id: &str) {
        let mut state = lock(&self.0.state);
        let generation = state.snapshot.generation;
        let Some(issue) = state.snapshot.issue.as_ref() else {
            return;
        };
        let Some(media) = issue
            .files
            .items
            .iter()
            .find(|m| m.media_id == media_id && m.can_remove_attachment())
        else {
            return;
        };
        if state.comment_holds_attachment(media) {
            return;
        }
        let media = media.clone();
        let Some(issue) = state.snapshot.issue.as_mut() else {
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
            if apply_result(
                &mut issue.files,
                &media.media_id,
                result,
                shared.client.attachment_protocol(),
            ) {
                super::reload_files(&shared, &mut state);
            }
            state.changed();
        });
    }
}

/// Records the outcome and returns whether to read the list again.
fn apply_result(
    files: &mut super::FilesSnapshot,
    id: &str,
    result: Result<(), UploadError>,
    protocol: AttachmentProtocol,
) -> bool {
    files.removing.remove(id);
    match result {
        Ok(()) => {
            files.items.retain(|media| media.media_id != id);
            // Supersede any page already fetching: its older snapshot could
            // otherwise put the deleted row back after the local removal.
            true
        }
        Err(UploadError::Refused { status: 409, .. }) => {
            files.notice = Some(protocol.conflict_notice());
            true
        }
        Err(_) => {
            files.notice = Some("Couldn't remove the attachment. Try again.");
            false
        }
    }
}
