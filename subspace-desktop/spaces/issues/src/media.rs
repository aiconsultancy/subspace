mod cache;
pub(crate) mod files;
mod plugin;
mod requests;
#[cfg(test)]
mod tests;
pub(crate) mod text;
pub(crate) mod view;
mod viewer;

use crate::Issues;
use gpui_kit::{Context, FocusHandle, RenderImage, ScrollAnchor, Task};
use std::{collections::HashMap, sync::Arc};
use subspace_api_client::{
    hub::{HubSnapshot, IssueStatus, ScopeGeneration},
    model::IssueMedia,
};

const MEMORY_LIMIT: usize = 128 * 1024 * 1024;
pub(crate) const VIEWER_CONTEXT: &str = "IssueImageViewer";
gpui_kit::actions!(
    issue_media,
    [
        PreviousImage,
        NextImage,
        CloseImage,
        NextImageControl,
        PreviousImageControl
    ]
);

pub(crate) fn bind_keys(cx: &mut gpui_kit::App) {
    cx.bind_keys([
        gpui_kit::KeyBinding::new("left", PreviousImage, Some(VIEWER_CONTEXT)),
        gpui_kit::KeyBinding::new("right", NextImage, Some(VIEWER_CONTEXT)),
        gpui_kit::KeyBinding::new("escape", CloseImage, Some(VIEWER_CONTEXT)),
        gpui_kit::KeyBinding::new("tab", NextImageControl, Some(VIEWER_CONTEXT)),
        gpui_kit::KeyBinding::new("shift-tab", PreviousImageControl, Some(VIEWER_CONTEXT)),
    ]);
}

#[derive(Clone)]
enum Preview {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
    TooLarge,
    Evicted,
}

pub(crate) struct MediaView {
    cache: cache::Cache<Preview>,
    tasks: HashMap<String, Task<()>>,
    scope: Option<ScopeGeneration>,
    issue: String,
    revision: Option<u64>,
    viewer: Option<viewer::Viewer>,
    /// The attachment panel beside the issue page.
    text: Option<text::TextViewer>,
    anchors: HashMap<String, ScrollAnchor>,
    focus: HashMap<String, FocusHandle>,
}
impl Default for MediaView {
    fn default() -> Self {
        Self {
            cache: cache::Cache::new(MEMORY_LIMIT, Preview::Evicted),
            tasks: HashMap::new(),
            scope: None,
            issue: String::new(),
            revision: None,
            viewer: None,
            text: None,
            anchors: HashMap::new(),
            focus: HashMap::new(),
        }
    }
}
fn key(media: &IssueMedia) -> String {
    format!("{}:{}", media.media_id, media.sha256)
}

impl MediaView {
    /// Whether the attachment panel is open beside the issue page.
    pub(crate) fn text_open(&self) -> bool {
        self.text.is_some()
    }

    fn open_issue(&mut self, issue: &str) {
        self.close_issue();
        self.issue = issue.into();
        self.cache.open(issue);
        self.cache
            .retain(|owner, preview| owner != issue || !matches!(preview, Preview::Evicted));
    }

    pub(crate) fn close_issue(&mut self) {
        for id in self.tasks.keys() {
            self.cache.remove(id);
        }
        self.tasks.clear();
        self.viewer = None;
        self.text = None;
        self.anchors.clear();
        self.focus.clear();
        self.issue.clear();
        self.revision = None;
    }

    fn follow_revision(&mut self, issue: &str, revision: u64) {
        if self.revision == Some(revision) {
            return;
        }
        self.cache.retain(|owner, preview| {
            owner != issue || !matches!(preview, Preview::Failed | Preview::Loading)
        });
        self.tasks.clear();
        self.revision = Some(revision);
    }

    pub(crate) fn is_open(&self) -> bool {
        self.viewer.is_some()
    }
    pub(crate) fn clear(&mut self) {
        self.close_issue();
        self.cache.clear();
        self.scope = None;
    }
}

// qual:allow(coupling, oi) reason: "The board owns image scope, task cancellation and the in-memory render cache at this seam."
impl Issues {
    pub(crate) fn follow_media(&mut self, snapshot: &HubSnapshot, cx: &mut Context<Self>) {
        if matches!(
            snapshot.status(),
            subspace_api_client::hub::HubStatus::Error(
                subspace_api_client::hub::ReadError::SignInNeeded
                    | subspace_api_client::hub::ReadError::Forbidden
            ) | subspace_api_client::hub::HubStatus::Stopped
        ) {
            self.media.clear();
            return;
        }
        if self.media.scope != Some(snapshot.generation()) {
            self.media.clear();
        }
        self.media.scope = Some(snapshot.generation());
        let Some(issue) = snapshot.issue() else {
            self.media.close_issue();
            return;
        };
        if !matches!(
            issue.status(),
            IssueStatus::Loaded
                | IssueStatus::Loading
                | IssueStatus::Error(subspace_api_client::hub::ReadError::Network)
        ) {
            self.media.clear();
            return;
        }
        if self.media.issue != issue.identifier() {
            self.media.open_issue(issue.identifier());
        }
        self.media
            .follow_revision(issue.identifier(), snapshot.issue_revision());
        if *issue.status() == IssueStatus::Loaded {
            self.read_image_anchor();
            self.media_anchors(snapshot, cx);
            if issue.images().is_idle()
                && let Some(hub) = &self.hub
            {
                let _ = hub.load_images(None);
            }
            if issue.files().is_idle()
                && let Some(hub) = &self.hub
            {
                _ = hub.load_attachments();
            }
            self.follow_viewer(issue);
            self.ensure_viewer_image(cx);
            self.read_text(cx);
        }
    }

    fn media_anchors(&mut self, snapshot: &HubSnapshot, cx: &Context<Self>) {
        let Some(issue) = snapshot.issue() else {
            return;
        };
        let media = issue
            .files()
            .items()
            .iter()
            .chain(
                issue
                    .timeline()
                    .page()
                    .into_iter()
                    .flat_map(|page| page.items.iter())
                    .filter_map(|entry| entry.comment.as_ref())
                    .flat_map(|comment| comment.media.iter()),
            )
            .chain(
                issue
                    .timeline()
                    .projected()
                    .iter()
                    .flat_map(|comment| &comment.media),
            )
            .chain(issue.images().items().iter().map(|image| &image.media));
        for item in media {
            self.media
                .focus
                .entry(format!("panel:{}", item.media_id))
                .or_insert_with(|| cx.focus_handle());
            self.media
                .focus
                .entry(item.media_id.clone())
                .or_insert_with(|| cx.focus_handle());
            self.media
                .anchors
                .entry(item.media_id.clone())
                .or_insert_with(|| ScrollAnchor::for_handle(self.issue_view.scroll.clone()));
        }
    }
}
