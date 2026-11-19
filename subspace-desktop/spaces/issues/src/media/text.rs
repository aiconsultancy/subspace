//! A side panel that shows one text or Markdown attachment of the open issue.
//! Bytes come from the Subspace API attachment read through the hub's paced
//! byte reads, never from render, and are prepared off the UI thread.
#[cfg(test)]
mod tests;
mod view;

pub(crate) use view::panel;

use crate::Issues;
use gpui_kit::{AppContext as _, Context, FocusHandle, SharedString, Task, Window};
use subspace_api_client::hub::ScopeGeneration;
use subspace_api_client::media::{MAX_TEXT_BYTES, readable_text};
use subspace_api_client::model::IssueMedia;

const MARKDOWN: &str = "text/markdown";
const BYTE_ORDER_MARK: char = '\u{feff}';
const CLOSED_WIDTH: f32 = 0.;

/// What the panel shows for its attachment.
#[derive(Clone)]
pub(crate) enum Content {
    Loading,
    /// Markdown prepared as the description is: no HTML, no remote images.
    Markdown(SharedString),
    Plain(SharedString),
    TooLarge,
    /// A 0-byte file: there is nothing to read.
    Empty,
    /// The read failed; Retry asks again.
    Failed,
    /// The bytes are not UTF-8 text.
    Unreadable,
}

/// What a read is for: the scope and issue revision it started in, and its
/// attachment. A result for any other is dropped.
#[derive(Clone, PartialEq, Eq)]
struct Fence {
    scope: ScopeGeneration,
    revision: u64,
    media_id: String,
}

/// The read the panel waits for.
struct Read {
    fence: Fence,
    _task: Task<()>,
}

pub(crate) struct TextViewer {
    pub(crate) media: IssueMedia,
    pub(crate) content: Content,
    pub(crate) close: FocusHandle,
    pub(crate) scroll: gpui_kit::ScrollHandle,
    return_to: Option<FocusHandle>,
    read: Option<Read>,
}

impl TextViewer {
    /// A loading or failed panel reads again once per issue revision.
    fn waiting(&self, scope: ScopeGeneration, revision: u64) -> bool {
        let current = self
            .read
            .as_ref()
            .is_some_and(|read| read.fence.scope == scope && read.fence.revision == revision);
        !current && matches!(self.content, Content::Loading | Content::Failed)
    }
}

/// The width the open panel takes from the page, as the shared detail
/// panel's: the stored panel width within 320 px and 75% of the window.
/// Zero while no attachment is open, so the page can lay out what is left.
pub(crate) fn panel_width(issues: &Issues, tokens: &subspace_framework::dashboard::Tokens) -> f32 {
    issues.media.text.as_ref().map_or(CLOSED_WIDTH, |_| {
        subspace_framework::dashboard::panel::clamp_width(
            f32::from(tokens.panel_width),
            issues.width,
        )
    })
}

/// Whether the panel can show this attachment.
pub(crate) fn viewable(media: &IssueMedia) -> bool {
    media.kind == "file" && readable_text(&media.content_type)
}

/// UTF-8 text, prepared for display: Markdown through the board's own
/// preparation, plain text as it is.
fn decode(bytes: Vec<u8>, markdown: bool) -> Content {
    let Ok(text) = String::from_utf8(bytes) else {
        return Content::Unreadable;
    };
    let text = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(&text);
    if markdown {
        Content::Markdown(prepare(text).into())
    } else {
        Content::Plain(text.to_owned().into())
    }
}

/// The description's preparation, with no media descriptors: an attachment
/// cannot draw the issue's images, and remote images stay links.
fn prepare(text: &str) -> String {
    crate::markdown::prepare_markdown(text, &crate::markdown::Options::new(&[], &[]))
}

// qual:allow(coupling, oi) reason: "The panel's commands coordinate the board's focus, the hub's fenced reads and the open issue's media scope."
impl Issues {
    pub(crate) fn open_text(
        &mut self,
        media: IssueMedia,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let close = cx.focus_handle();
        self.media.text = Some(TextViewer {
            media,
            content: Content::Loading,
            close: close.clone(),
            scroll: gpui_kit::ScrollHandle::new(),
            return_to: window.focused(cx),
            read: None,
        });
        close.focus(window, cx);
        // The Close button tracks the handle once the panel has rendered.
        cx.defer_in(window, |this, window, cx| {
            if let Some(viewer) = &this.media.text {
                viewer.close.focus(window, cx);
            }
        });
        self.read_text(cx);
        cx.notify();
    }

    /// Closes the panel and returns focus to its opener. False when no
    /// panel was open.
    pub(crate) fn close_text(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(viewer) = self.media.text.take() else {
            return false;
        };
        viewer
            .return_to
            .unwrap_or_else(|| self.focus.clone())
            .focus(window, cx);
        cx.notify();
        true
    }

    pub(crate) fn retry_text(&mut self, cx: &mut Context<Self>) {
        if let Some(viewer) = self.media.text.as_mut() {
            viewer.read = None;
        }
        self.read_text(cx);
        cx.notify();
    }

    /// Starts the read the open panel waits for. Empty and oversized
    /// attachments are settled before any request, as image previews are.
    pub(crate) fn read_text(&mut self, cx: &mut Context<Self>) {
        let (Some(scope), Some(revision), Some(hub)) =
            (self.media.scope, self.media.revision, self.hub.clone())
        else {
            return;
        };
        let Some(viewer) = self.media.text.as_mut() else {
            return;
        };
        if !viewer.waiting(scope, revision) {
            return;
        }
        if let Some(content) = settled(&viewer.media) {
            viewer.content = content;
            return;
        }
        viewer.content = Content::Loading;
        let media = viewer.media.clone();
        let markdown = media.content_type == MARKDOWN;
        let fence = Fence {
            scope,
            revision,
            media_id: media.media_id.clone(),
        };
        let answer = fence.clone();
        let task = cx.spawn(async move |this, cx| {
            let content = match hub.fetch_text(scope, revision, media, MAX_TEXT_BYTES).await {
                Ok(bytes) => {
                    cx.background_spawn(async move { decode(bytes, markdown) })
                        .await
                }
                Err(_) => Content::Failed,
            };
            _ = this.update(cx, |this, cx| this.finish_text(&answer, content, cx));
        });
        viewer.read = Some(Read { fence, _task: task });
    }

    /// Shows a read's result only while the board is still in that read's
    /// scope and issue revision, as image previews are, and the panel still
    /// waits for that read of that attachment. A dropped result leaves the
    /// panel loading, so it reads again once the new revision loads.
    fn finish_text(&mut self, fence: &Fence, content: Content, cx: &mut Context<Self>) {
        let current =
            self.media.scope == Some(fence.scope) && self.media.revision == Some(fence.revision);
        let Some(viewer) = self.media.text.as_mut() else {
            return;
        };
        if current
            && viewer
                .read
                .as_ref()
                .is_some_and(|read| read.fence == *fence)
        {
            viewer.content = content;
            cx.notify();
        }
    }
}

/// What the panel shows with no request: an empty file has nothing to read,
/// and one over the limit is refused.
fn settled(media: &IssueMedia) -> Option<Content> {
    match media.size_bytes {
        ..=0 => Some(Content::Empty),
        size if size > MAX_TEXT_BYTES as i64 => Some(Content::TooLarge),
        _ => None,
    }
}
