//! File bytes: image, text and standalone file reads share the paced API
//! workers and bounded admission.
use crate::hub::{
    Hub, ReadPriority, Shared, lock,
    worker::{Job, Task},
};
use crate::{Error, model::IssueMedia};
use tokio::sync::oneshot;

/// Which read a byte request uses: image bytes, a text attachment or any
/// standalone file.
#[derive(Clone, Copy)]
pub(in crate::hub) enum ByteRead {
    Image,
    Text,
    File,
}

pub(in crate::hub) struct FileRead {
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
            ByteRead::File => client.fetch_file(&self.media, self.limit),
        }
    }
}

impl Hub {
    pub async fn fetch_image(
        &self,
        generation: crate::hub::ScopeGeneration,
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
        generation: crate::hub::ScopeGeneration,
        revision: u64,
        media: IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        self.fetch_bytes((generation, revision), (media, limit), ByteRead::Text)
            .await
    }

    /// A standalone attachment's bytes, of any type, for saving.
    pub async fn fetch_file(
        &self,
        generation: crate::hub::ScopeGeneration,
        revision: u64,
        media: IssueMedia,
        limit: usize,
    ) -> Result<Vec<u8>, Error> {
        self.fetch_bytes((generation, revision), (media, limit), ByteRead::File)
            .await
    }

    async fn fetch_bytes(
        &self,
        (generation, revision): (crate::hub::ScopeGeneration, u64),
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
}

pub(in crate::hub) fn bytes(shared: &Shared, job: &Job, request: &FileRead) {
    if lock(&request.reply)
        .as_ref()
        .is_none_or(|reply| reply.is_closed())
    {
        return;
    }
    let sent = std::time::Instant::now();
    let result = request.fetch(&shared.client);
    // A NETT token refused before it expires is renewed rather than shown
    // as signed out; the byte read itself still fails.
    let refused = crate::hub::renewal::refused(shared, job.generation, sent, &result);
    let mut state = lock(&shared.state);
    let result = if !job.current(&state) {
        Err(Error::InvalidRequest)
    } else {
        result
    };
    if refused && matches!(result, Err(Error::SignInNeeded | Error::Forbidden)) {
        state.snapshot.status = crate::hub::HubStatus::Error(
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
