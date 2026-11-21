//! What the store asks of an upload: whether an attempt is under way,
//! whether a closed issue keeps it, the reservation to cancel as it goes,
//! and its bytes let go or left out of a snapshot.
use super::{AttachmentUpload, Cancel};
use crate::hub::AttachmentState;

impl AttachmentUpload {
    /// Lets go of its bytes and keeps the rest: a check needs no bytes, and
    /// sending again then needs the file chosen again.
    pub(super) fn release_bytes(&mut self) {
        self.bytes = std::sync::Arc::from(Vec::new());
        self.released = true;
    }

    /// This upload as a snapshot shows it: everything but its bytes, which
    /// only the store and an attempt under way hold, so a snapshot kept
    /// after an upload goes never keeps its bytes from being freed.
    pub(super) fn without_bytes(&self) -> Self {
        Self {
            bytes: std::sync::Arc::from(Vec::new()),
            ..self.clone()
        }
    }

    /// Whether an attempt is under way, or queued: its end settles the row.
    pub(in crate::hub) fn in_flight(&self) -> bool {
        !matches!(
            self.state,
            AttachmentState::Failed(_) | AttachmentState::NotFound
        )
    }

    /// Whether a closed issue keeps this upload: an attempt is under way, or
    /// bytes that may be stored wait for a check that can still change the
    /// outcome. A failure with nothing to check is let go.
    pub(in crate::hub) fn worth_keeping(&self) -> bool {
        match self.state {
            AttachmentState::Failed(_) | AttachmentState::NotFound => {
                self.checks() && self.retryable()
            }
            _ => true,
        }
    }

    /// The reservation to cancel when this upload is let go, under the ID
    /// of its `issue`. One under way cancels its own when its attempt ends
    /// and finds no row.
    pub(in crate::hub) fn cancel_on(&self, issue: &str) -> Option<Cancel> {
        self.transfer
            .clone()
            .filter(|_| !self.in_flight())
            .map(|transfer| (issue.to_owned(), transfer))
    }
}
