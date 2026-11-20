//! Removing a standalone attachment at the version listed.
use crate::hub::{Hub, lock};
use crate::uploads::UploadError;
use std::sync::Arc;

const CHANGED_ELSEWHERE: &str = "This attachment changed elsewhere. The list was read again.";

impl Hub {
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
                super::reload_files(&shared, &mut state);
            }
            state.changed();
        });
    }
}
