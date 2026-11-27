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
use crate::{
    PageRequest,
    model::{IssueMedia, Page},
    uploads::AttachmentProtocol,
};
pub(super) const QUEUE_CAPACITY: usize = 100;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Endpoint {
    #[default]
    Unknown,
    Listed,
    Missing,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Read {
    #[default]
    Idle,
    Loading,
    Loaded,
    Failed,
}

#[derive(Clone, Default)]
// qual:allow(srp, god_struct) reason: "One immutable snapshot exposes independently readable media metadata and load status."
pub struct FilesSnapshot {
    items: Vec<IssueMedia>,
    cursor: Option<String>,
    read: Read,
    endpoint: Endpoint,
    scan: u64,
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
            endpoint: match self.endpoint {
                Endpoint::Listed => Endpoint::Listed,
                Endpoint::Unknown | Endpoint::Missing => Endpoint::Unknown,
            },
            scan: self.scan.wrapping_add(1),
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
        self.read == Read::Idle
    }
    /// Whether the panel can offer standalone file actions.
    pub fn accepts_files(&self, protocol: AttachmentProtocol) -> bool {
        protocol.accepts_files(self.endpoint == Endpoint::Listed)
    }
    pub fn is_failed(&self) -> bool {
        self.read == Read::Failed
    }
    pub fn is_loading(&self) -> bool {
        self.read == Read::Loading
    }
    pub fn has_more(&self) -> bool {
        self.cursor.is_some()
    }

    fn absorb(&mut self, page: Page<IssueMedia>) {
        self.endpoint = Endpoint::Listed;
        self.rescan.absorb(
            &mut self.items,
            page.items,
            page.next_cursor.is_none(),
            |item| &item.media_id,
        );
        self.read = match page.next_cursor.as_ref() {
            Some(next) if !self.cursors.insert(next.clone()) => Read::Failed,
            _ => Read::Loaded,
        };
        self.cursor = page.next_cursor;
    }
}

impl Hub {
    pub fn attachment_protocol(&self) -> AttachmentProtocol {
        self.0.client.attachment_protocol()
    }

    // qual:allow(dry, fragment) reason: "The UI boundary only locks state and queues a navigation-fenced read."
    pub fn load_attachments(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state.snapshot.issue.as_mut().filter(|i| i.detail.is_some()) else {
            return Ok(());
        };
        let files = &mut issue.files;
        if matches!(files.read, Read::Loading | Read::Loaded) {
            return Ok(());
        }
        files.read = Read::Loading;
        let task = Task::Files(issue.identifier.clone(), files.cursor.clone());
        state.changed();
        self.0.enqueue(&mut state, task, ReadPriority::Visible);
        Ok(())
    }
}

pub(super) fn load(shared: &Shared, job: &Job, issue: &str, cursor: Option<&str>) {
    let Some(scan) = current_scan(shared, cursor) else {
        return;
    };
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
    if !files.is_loading() || files.scan != scan || files.cursor.as_deref() != cursor {
        return;
    }
    match result {
        Ok(page) => files.absorb(page),
        Err(error) if shared.client.attachment_protocol().hidden_endpoint(error) => {
            files.endpoint = Endpoint::Missing;
            files.read = Read::Loaded;
            files.cursor = None;
            files.rescan.abandon();
        }
        Err(error) => {
            files.read = Read::Failed;
            files.rescan.abandon();
            if refused {
                state.snapshot.status = super::HubStatus::Error(error.into());
                state.changed();
                return;
            }
        }
    }
    if files.has_more() && !files.is_failed() {
        files.read = Read::Loading;
        let task = Task::Files(issue.identifier.clone(), files.cursor.clone());
        shared.enqueue(&mut state, task, ReadPriority::LaterPage);
    }
    state.changed();
}

fn current_scan(shared: &Shared, cursor: Option<&str>) -> Option<u64> {
    let state = lock(&shared.state);
    let files = &state.snapshot.issue.as_ref()?.files;
    (files.is_loading() && files.cursor.as_deref() == cursor).then_some(files.scan)
}

/// Reads the open issue's attachment list again from its first page.
fn reload_files(shared: &Shared, state: &mut super::State) {
    let Some(issue) = state.snapshot.issue.as_mut() else {
        return;
    };
    issue.files.reload();
    issue.files.read = Read::Loading;
    let task = Task::Files(issue.identifier.clone(), None);
    shared.enqueue(state, task, ReadPriority::Visible);
}
