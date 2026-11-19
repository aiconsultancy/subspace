//! Byte requests share the paced API workers and bounded admission.
use super::{
    Hub, HubError, ReadPriority, Shared, lock,
    worker::{Job, Task},
};
use crate::{Error, PageRequest, model::IssueMedia};
use tokio::sync::oneshot;
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
}
impl FilesSnapshot {
    /// Reads the list again from its first page, keeping what is shown
    /// until a complete scan says what is gone.
    pub(in crate::hub) fn reload(&mut self) {
        *self = Self {
            items: std::mem::take(&mut self.items),
            ..Self::default()
        };
        self.rescan.begin();
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

/// Which read a byte request uses: image bytes or a text attachment.
#[derive(Clone, Copy)]
pub(super) enum ByteRead {
    Image,
    Text,
}

pub(super) struct FileRead {
    pub media: IssueMedia,
    pub limit: usize,
    pub read: ByteRead,
    pub reply: std::sync::Mutex<Option<oneshot::Sender<Result<Vec<u8>, Error>>>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl FileRead {
    fn fetch(&self, client: &crate::Client) -> Result<Vec<u8>, Error> {
        match self.read {
            ByteRead::Image => client.fetch_image(&self.media, self.limit),
            ByteRead::Text => client.fetch_text(&self.media, self.limit),
        }
    }
}

impl Hub {
    pub async fn fetch_image(
        &self,
        generation: super::ScopeGeneration,
        revision: u64,
        media: IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        self.fetch_bytes((generation, revision), (media, limit), ByteRead::Image)
            .await
    }

    /// A text attachment's bytes, through the same paced, bounded byte reads
    /// as images.
    pub async fn fetch_text(
        &self,
        generation: super::ScopeGeneration,
        revision: u64,
        media: IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        self.fetch_bytes((generation, revision), (media, limit), ByteRead::Text)
            .await
    }

    async fn fetch_bytes(
        &self,
        (generation, revision): (super::ScopeGeneration, u64),
        (media, limit): (IssueMedia, usize),
        read: ByteRead,
    ) -> Result<Vec<u8>, Error> {
        let permit = self
            .0
            .file_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::Network)?;
        let (reply, receive) = oneshot::channel();
        {
            let mut state = lock(&self.0.state);
            if !state.issue_current(generation, revision) {
                return Err(Error::InvalidRequest);
            }
            self.0.enqueue(
                &mut state,
                Task::MediaBytes(FileRead {
                    media,
                    limit,
                    read,
                    reply: std::sync::Mutex::new(Some(reply)),
                    _permit: permit,
                }),
                ReadPriority::Visible,
            );
        }
        receive.await.unwrap_or(Err(Error::Network))
    }

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

pub(super) fn bytes(shared: &Shared, job: &Job, request: &FileRead) {
    if lock(&request.reply)
        .as_ref()
        .is_none_or(|reply| reply.is_closed())
    {
        return;
    }
    let result = request.fetch(&shared.client);
    let mut state = lock(&shared.state);
    let result = if !job.current(&state) {
        Err(Error::InvalidRequest)
    } else {
        result
    };
    if matches!(result, Err(Error::SignInNeeded | Error::Forbidden)) {
        state.snapshot.status = super::HubStatus::Error(
            result
                .as_ref()
                .err()
                .copied()
                .unwrap_or(Error::Forbidden)
                .into(),
        );
        state.changed();
    }
    if let Some(reply) = lock(&request.reply).take() {
        _ = reply.send(result);
    }
}

pub(super) fn load(shared: &Shared, job: &Job, issue: &str, cursor: Option<&str>) {
    let result = shared.client.attachments(
        issue,
        &PageRequest {
            cursor: cursor.map(str::to_owned),
            ..PageRequest::default()
        },
    );
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
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
            if matches!(error, Error::SignInNeeded | Error::Forbidden) {
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
