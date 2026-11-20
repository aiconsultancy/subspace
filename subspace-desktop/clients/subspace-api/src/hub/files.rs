//! An open issue's attachments: the list, read page by page, its files'
//! bytes, and the standalone attachments added to it or removed from it.
mod bytes;
mod removal;
mod uploads;

pub(super) use bytes::{FileRead, bytes};
#[cfg(test)]
pub(super) use uploads::UploadIssue;
pub(super) use uploads::UploadStore;
pub use uploads::{AttachmentTarget, AttachmentUpload};

use super::{
    Hub, HubError, ReadPriority, Shared, lock,
    worker::{Job, Task},
};
use crate::{PageRequest, model::IssueMedia};
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
    /// Standalone attachments being added from the Attachments panel, in a
    /// snapshot handed out; see [`FilesSnapshot::show_uploads`].
    uploads: Vec<AttachmentUpload>,
    /// Attachments being removed, by media ID.
    removing: std::collections::HashSet<String>,
    /// What happened to the last add or remove, when it needs saying.
    notice: Option<&'static str>,
}

impl FilesSnapshot {
    /// Reads the list again from its first page, keeping what is shown
    /// until a complete scan says what is gone. Uploads live in the hub's
    /// upload store, not here.
    pub(in crate::hub) fn reload(&mut self) {
        *self = Self {
            items: std::mem::take(&mut self.items),
            removing: std::mem::take(&mut self.removing),
            notice: self.notice.take(),
            ..Self::default()
        };
        self.rescan.begin();
    }
    /// Forgets the list and its read state. Uploads in progress stay in the
    /// hub's upload store.
    pub(in crate::hub) fn clear(&mut self) {
        *self = Self::default();
    }
    /// Shows `uploads`, the open issue's from the upload store, in a
    /// snapshot handed out.
    // qual:allow(dry, boilerplate) reason: "Only the hub composes a handed-out snapshot's uploads from its store."
    pub(in crate::hub) fn show_uploads(&mut self, uploads: Vec<AttachmentUpload>) {
        self.uploads = uploads;
    }
    /// Standalone attachments being added.
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
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

impl Hub {
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

pub(super) fn load(shared: &Shared, job: &Job, issue: &str, cursor: Option<&str>) {
    let sent = std::time::Instant::now();
    let result = shared.client.attachments(
        issue,
        &PageRequest {
            cursor: cursor.map(str::to_owned),
            ..PageRequest::default()
        },
    );
    let Some((mut state, refused)) = super::renewal::read_back(shared, job, sent, &result) else {
        return;
    };
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
