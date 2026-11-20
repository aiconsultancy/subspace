//! Cursor metadata is retained with the issue; reads are navigation-fenced.
use super::{
    Hub, HubError, ReadPriority, Shared, lock,
    worker::{Job, Task},
};
use crate::{PageRequest, media::IssueImage};

#[derive(Debug, Clone, Default)]
pub struct ImagesSnapshot {
    items: Vec<IssueImage>,
    cursor: Option<String>,
    loaded: bool,
    loading: bool,
    failed: bool,
    anchor: Option<String>,
    empty_followed: bool,
    cursors: std::collections::HashSet<String>,
    rescan: super::rescan::Rescan,
}

// qual:allow(dry, boilerplate) reason: "Readers preserve immutable hub snapshots across the client boundary."
impl ImagesSnapshot {
    pub fn items(&self) -> &[IssueImage] {
        &self.items
    }
    pub fn has_more(&self) -> bool {
        self.cursor.is_some()
    }
    pub fn is_partial(&self) -> bool {
        self.has_more()
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn is_loading(&self) -> bool {
        self.loading
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary."
    pub fn is_failed(&self) -> bool {
        self.failed
    }
    pub fn is_idle(&self) -> bool {
        !self.loaded && !self.loading && !self.failed
    }
    pub fn is_visible(&self) -> bool {
        !self.loaded || !self.items.is_empty() || self.has_more() || self.failed
    }
    pub fn anchor_id(&self) -> Option<&str> {
        self.anchor.as_deref()
    }
    pub fn is_anchor_gone(&self) -> bool {
        self.loaded
            && !self.has_more()
            && self
                .anchor
                .as_ref()
                .is_some_and(|id| !self.items.iter().any(|item| &item.media.media_id == id))
    }
    /// Reads the list again from its first page, keeping what is shown
    /// until a complete scan says what is gone.
    fn reload(&mut self) {
        *self = Self {
            items: std::mem::take(&mut self.items),
            ..Self::default()
        };
        self.rescan.begin();
    }

    fn install(&mut self, page: crate::model::Page<crate::media::ImageMetadata>) {
        let incoming = page
            .items
            .into_iter()
            .map(|metadata| {
                let media = metadata.descriptor();
                IssueImage {
                    alt_text: media.file_name.clone(),
                    media,
                    comment_id: None,
                }
            })
            .collect();
        self.rescan.absorb(
            &mut self.items,
            incoming,
            page.next_cursor.is_none(),
            |image| &image.media.media_id,
        );
        self.failed = page
            .next_cursor
            .as_ref()
            .is_some_and(|next| !self.cursors.insert(next.clone()));
        self.cursor = page.next_cursor;
        self.loaded = true;
    }
}

impl Hub {
    // qual:allow(dry, fragment) reason: "The UI boundary only locks state and queues a navigation-fenced read."
    pub fn load_images(&self, around: Option<&str>) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        let Some(issue) = state.snapshot.issue.as_mut().filter(|i| i.detail.is_some()) else {
            return Ok(());
        };
        let images = &mut issue.images;
        if images.loading {
            return Ok(());
        }
        if let Some(id) = around {
            images.anchor = Some(id.into());
        }
        if images.loaded && !images.has_more() && !images.failed {
            return Ok(());
        }
        images.loading = true;
        images.failed = false;
        let task = Task::Images(issue.identifier.clone(), images.cursor.clone());
        state.changed();
        self.0.enqueue(&mut state, task, ReadPriority::Visible);
        Ok(())
    }
}

pub(super) fn load(shared: &Shared, job: &Job, issue: &str, cursor: Option<&str>) {
    let page = PageRequest {
        cursor: cursor.map(str::to_owned),
        ..PageRequest::default()
    };
    let sent = std::time::Instant::now();
    let result = shared.client.issue_images(issue, &page);
    // A NETT token refused before it expires is renewed rather than shown
    // as signed out.
    let refused = match result {
        Err(error @ (crate::Error::SignInNeeded | crate::Error::Forbidden)) => {
            super::renewal::stands(shared, job.generation, sent, error)
        }
        _ => false,
    };
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    let Some(issue) = state.snapshot.issue.as_mut() else {
        return;
    };
    let images = &mut issue.images;
    images.loading = false;
    match result {
        Ok(page) => images.install(page),
        Err(error) => {
            images.failed = true;
            images.rescan.abandon();
            if refused {
                state.snapshot.status = super::HubStatus::Error(error.into());
                state.changed();
                return;
            }
        }
    }
    let follow = !images.failed
        && images.has_more()
        && ((images.items.is_empty() && !images.empty_followed)
            || images
                .anchor
                .as_ref()
                .is_some_and(|id| !images.items.iter().any(|i| &i.media.media_id == id)));
    if follow {
        images.loading = true;
        images.empty_followed |= images.items.is_empty();
        let task = Task::Images(issue.identifier.clone(), images.cursor.clone());
        shared.enqueue(&mut state, task, ReadPriority::LaterPage);
    }
    state.changed();
}

impl super::State {
    pub(in crate::hub) fn refresh_media(&mut self, identifier: &str) {
        let Some(issue) = self.snapshot.issue.as_mut().filter(|issue| {
            issue.detail.as_ref().is_some_and(|detail| {
                detail.fields.id == identifier || detail.fields.identifier == identifier
            })
        }) else {
            return;
        };
        issue.images.reload();
        issue.files.reload();
        self.media_revision = self.media_revision.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use crate::hub::{
        IssueSnapshot, IssueStatus, State,
        worker::{Job, Task},
    };
    use serde_json::json;
    #[test]
    fn confirmed_upload_refresh_retires_metadata_pages_and_preserves_detail_reads() {
        let mut state = State::new("http://127.0.0.1/api", "Fixture");
        let mut value = crate::tests::row("1");
        value["todos"] = json!([]);
        let mut issue = IssueSnapshot::new("SUB-1", crate::PROFILE_ID, IssueStatus::Loaded);
        issue.detail = Some(serde_json::from_value(value).unwrap());
        state.snapshot.issue = Some(issue);
        let metadata = Job::new(&state, Task::Images("SUB-1".into(), None));
        let detail = Job::new(&state, Task::Issue("SUB-1".into()));
        state.refresh_media("1");
        assert!(!metadata.current(&state));
        assert!(detail.current(&state));
        assert!(state.snapshot.issue.as_ref().unwrap().images().is_idle());
    }
}
