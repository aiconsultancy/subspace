//! Sending a file again by the person's explicit choice, when checks in a
//! row could not tell whether NETT stored it.
use super::{AttachmentUpload, sending};
use crate::hub::{AttachmentState, Hub, lock};
use std::sync::Arc;

impl Hub {
    /// Sends the open issue's upload `id` again under a new key, as the
    /// person chose once checks in a row could not tell whether NETT
    /// stored its file, knowing it may then be attached twice. Its earlier
    /// reservation is left to lapse rather than cancelled: its completion
    /// may have landed.
    pub fn send_attachment_upload_again(&self, id: &str) {
        let mut state = lock(&self.0.state);
        self.0.follow_uploads(&mut state);
        let generation = state.snapshot.generation;
        let open = state.open_for_uploads();
        let Some((upload, issue)) = state
            .uploads
            .shown_row(open.as_ref(), id)
            .filter(|(upload, _)| upload.may_send_again())
        else {
            return;
        };
        upload.start_again();
        let attempt = sending::Attempt::new(issue.clone());
        let upload = upload.clone();
        state.changed();
        let shared = Arc::clone(&self.0);
        sending::run(move || sending::send(&shared, generation, &attempt, &upload));
    }
}

impl AttachmentUpload {
    /// Starts afresh, as a new upload of the same bytes under a new key.
    fn start_again(&mut self) {
        self.request_key = uuid::Uuid::new_v4().to_string();
        self.transfer = None;
        self.bytes_sent = false;
        self.unknown_checks = 0;
        self.retry_at = None;
        self.state = AttachmentState::Uploading;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl AttachmentUpload {
    /// An upload of `file_name` in `state`, for tests of how it is shown:
    /// whether its bytes may be stored (`checks`), how many checks in a row
    /// could not tell, and whether its bytes were let go.
    // qual:test_helper
    pub fn for_test(
        file_name: &str,
        state: AttachmentState,
        checks: bool,
        unknown_checks: u8,
        released: bool,
    ) -> Result<Self, crate::uploads::UploadError> {
        let bytes = b"Fixture file.\n".to_vec();
        let declaration = crate::uploads::ImageDeclaration::new(file_name, "text/plain", &bytes)?;
        let mut upload = Self::new(declaration, bytes.into());
        upload.state = state;
        upload.transfer = checks.then(|| "transfer-fixture".to_owned());
        upload.bytes_sent = checks;
        upload.unknown_checks = unknown_checks;
        upload.released = released;
        Ok(upload)
    }
}

#[cfg(test)]
mod again_tests;
