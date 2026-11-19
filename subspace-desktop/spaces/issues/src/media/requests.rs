//! Preview admission and fetch/decode lifecycle for the current issue.
use super::{MediaView, Preview, key};
use crate::Issues;
use gpui_kit::{AppContext as _, Context};
use subspace_api_client::uploads::{
    MAX_IMAGE_BYTES as MAX_ENCODED_BYTES, MAX_IMAGE_EDGE, MAX_IMAGE_PIXELS,
};
use subspace_api_client::{
    hub::{Hub, ScopeGeneration},
    model::IssueMedia,
};
use subspace_framework::dashboard::images::{DecodeLimits, decode_with_limits};

const TILE_REQUESTS: usize = 3;
const VIEWER_REQUESTS: usize = TILE_REQUESTS + 1;
const BYTES_PER_PIXEL: usize = 4;

struct Request {
    issue: String,
    id: String,
    scope: ScopeGeneration,
    revision: u64,
}

impl MediaView {
    fn viewer_key(&self) -> Option<String> {
        self.viewer.as_ref().map(|viewer| key(&viewer.media))
    }

    fn can_request(&self, id: &str) -> bool {
        let viewer = self.viewer_key().as_deref() == Some(id);
        let capacity = if viewer {
            VIEWER_REQUESTS
        } else {
            TILE_REQUESTS
        };
        self.tasks.len() < capacity
            && match self.cache.get(id) {
                None => true,
                Some(Preview::Evicted) => viewer,
                _ => false,
            }
    }

    fn request(&self, id: String) -> Option<Request> {
        if !self.can_request(&id) {
            return None;
        }
        Some(Request {
            id,
            issue: self.issue.clone(),
            scope: self.scope?,
            revision: self.revision?,
        })
    }

    fn record(&mut self, request: &Request, preview: Preview) {
        let keep = self.viewer_key();
        let bytes = preview_bytes(&preview);
        self.cache.insert(
            &request.id,
            &request.issue,
            (preview, bytes),
            keep.as_deref(),
        );
    }
}

// qual:allow(coupling, oi) reason: "Image tasks coordinate board state and the async hub admission boundary."
impl Issues {
    pub(super) fn ensure_image(&mut self, media: IssueMedia, cx: &mut Context<Self>) {
        let Some(request) = self.media.request(key(&media)) else {
            return;
        };
        if media.size_bytes > MAX_ENCODED_BYTES as i64 {
            self.media.record(&request, Preview::TooLarge);
            cx.notify();
            return;
        }
        if let Some(hub) = self.hub.clone() {
            self.start_image(hub, media, request, cx);
        }
    }

    fn start_image(
        &mut self,
        hub: Hub,
        media: IssueMedia,
        request: Request,
        cx: &mut Context<Self>,
    ) {
        self.media.record(&request, Preview::Loading);
        let id = request.id.clone();
        let content_type = media.content_type.clone();
        let task = cx.spawn(async move |this, cx| {
            let preview = match hub
                .fetch_image(
                    request.scope,
                    request.revision,
                    media.clone(),
                    MAX_ENCODED_BYTES,
                )
                .await
            {
                Ok(bytes) => {
                    cx.background_spawn(async move { decode_preview(&bytes, &content_type) })
                        .await
                }
                Err(_) => Preview::Failed,
            };
            let _ = this.update(cx, |this, cx| this.finish_image(&request, preview, cx));
        });
        self.media.tasks.insert(id, task);
    }

    fn finish_image(&mut self, request: &Request, preview: Preview, cx: &mut Context<Self>) {
        if self.media_current(request) {
            self.media.record(request, preview);
            self.media.tasks.remove(&request.id);
            self.ensure_viewer_image(cx);
            // Let visible tiles ask again now that a slot is free.
            cx.notify();
        }
    }

    fn media_current(&self, request: &Request) -> bool {
        self.media.scope == Some(request.scope)
            && self.media.issue == request.issue
            && self.media.revision == Some(request.revision)
            && self.hub.as_ref().is_some_and(|hub| {
                let current = hub.snapshot();
                current.generation() == request.scope
                    && current.issue_revision() == request.revision
            })
    }

    pub(super) fn ensure_viewer_image(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = &self.media.viewer {
            self.ensure_image(viewer.media.clone(), cx);
        }
    }

    pub(super) fn retry_image(&mut self, media: IssueMedia, cx: &mut Context<Self>) {
        self.media.cache.remove(&key(&media));
        self.ensure_image(media, cx);
        // A retry that is waiting for a slot needs a fresh visible tile.
        cx.notify();
    }
}

fn decode_preview(bytes: &[u8], content_type: &str) -> Preview {
    match decode_with_limits(
        bytes,
        content_type,
        DecodeLimits {
            encoded_bytes: MAX_ENCODED_BYTES,
            dimension: MAX_IMAGE_EDGE,
            pixels: MAX_IMAGE_PIXELS,
        },
    ) {
        Ok(image) => Preview::Ready(image),
        Err(error) if error.contains("exceeds") => Preview::TooLarge,
        Err(_) => Preview::Failed,
    }
}

fn preview_bytes(preview: &Preview) -> usize {
    match preview {
        Preview::Ready(image) => {
            let size = image.size(0);
            size.width.0 as usize * size.height.0 as usize * BYTES_PER_PIXEL
        }
        _ => 0,
    }
}
