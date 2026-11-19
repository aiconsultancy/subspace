mod view;
use crate::Issues;
use gpui_kit::{Context, FocusHandle, Window};
use subspace_api_client::{hub::IssueSnapshot, media::IssueImage, model::IssueMedia};
pub(crate) use view::render;

#[derive(Clone, Copy)]
pub(super) struct Position {
    index: usize,
    count: usize,
    more: bool,
    partial: bool,
}

pub(super) struct Viewer {
    pub media: IssueMedia,
    pub alt: String,
    pub comment: Option<String>,
    pub close: FocusHandle,
    pub previous: FocusHandle,
    pub next: FocusHandle,
    pub show: FocusHandle,
    pub return_to: Option<FocusHandle>,
    pub waiting_next: bool,
    pub alone: bool,
    pub pending_anchor: Option<String>,
    pub requested_anchor: bool,
}

impl Viewer {
    fn position(&self, issue: &IssueSnapshot) -> Option<Position> {
        let images = issue.images();
        let (index, count, more, partial) = if self.alone {
            (0, 1, false, false)
        } else {
            let index = images
                .items()
                .iter()
                .position(|image| image.media.media_id == self.media.media_id)?;
            (
                index,
                images.items().len(),
                images.has_more(),
                images.is_partial(),
            )
        };
        Some(Position {
            index,
            count,
            more,
            partial,
        })
    }
}

// qual:allow(coupling, oi) reason: "Viewer commands coordinate the board's navigation, focus and image task owner."
impl Issues {
    pub(crate) fn open_image(
        &mut self,
        media: IssueMedia,
        alt: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let occurrence = self
            .issue_view
            .snapshot
            .as_ref()
            .and_then(|issue| {
                issue
                    .images()
                    .items()
                    .iter()
                    .find(|image| image.media.media_id == media.media_id)
            })
            .cloned();
        let close = cx.focus_handle();
        let viewer = Viewer {
            media: media.clone(),
            alt,
            comment: occurrence
                .as_ref()
                .and_then(|image| image.comment_id.clone())
                .or_else(|| {
                    self.issue_view
                        .snapshot
                        .as_ref()
                        .and_then(|issue| comment_source(issue, &media.media_id))
                }),
            close: close.clone(),
            previous: cx.focus_handle(),
            next: cx.focus_handle(),
            show: cx.focus_handle(),
            return_to: window.focused(cx),
            waiting_next: false,
            alone: false,
            pending_anchor: occurrence.is_none().then(|| media.media_id.clone()),
            requested_anchor: occurrence.is_none(),
        };
        self.media.viewer = Some(viewer);
        close.focus(window, cx);
        cx.defer_in(window, |this, window, cx| {
            if let Some(viewer) = &this.media.viewer {
                viewer.close.focus(window, cx);
            }
        });
        self.read_image_anchor();
        self.ensure_image(media, cx);
        cx.notify();
    }

    pub(crate) fn close_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(viewer) = self.media.viewer.take() {
            viewer
                .return_to
                .unwrap_or_else(|| self.focus.clone())
                .focus(window, cx);
            cx.notify();
        }
    }

    pub(crate) fn step_image(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(viewer) = self.media.viewer.as_mut() else {
            return;
        };
        if viewer.alone {
            return;
        }
        let Some(images) = self
            .issue_view
            .snapshot
            .as_ref()
            .map(|issue| issue.images())
        else {
            return;
        };
        let Some(ix) = images
            .items()
            .iter()
            .position(|image| image.media.media_id == viewer.media.media_id)
        else {
            return;
        };
        let target = if forward {
            ix.checked_add(1)
        } else {
            ix.checked_sub(1)
        };
        if let Some(image) = target.and_then(|ix| images.items().get(ix)).cloned() {
            self.select_image(image, cx);
        } else if forward && images.has_more() {
            viewer.waiting_next = true;
            if let Some(hub) = &self.hub {
                let _ = hub.load_images(None);
            }
        }
        cx.notify();
    }

    fn select_image(&mut self, image: IssueImage, cx: &mut Context<Self>) {
        if let Some(viewer) = self.media.viewer.as_mut() {
            viewer.media = image.media.clone();
            viewer.alt = image.alt_text;
            viewer.comment = image.comment_id.or_else(|| {
                self.issue_view
                    .snapshot
                    .as_ref()
                    .and_then(|issue| comment_source(issue, &image.media.media_id))
            });
            viewer.waiting_next = false;
        }
        self.ensure_image(image.media, cx);
    }

    pub(crate) fn follow_viewer(&mut self, issue: &IssueSnapshot) {
        let Some(viewer) = self.media.viewer.as_mut() else {
            return;
        };
        viewer.alone |= viewer.requested_anchor
            && issue.images().is_anchor_gone()
            && issue.images().anchor_id() == Some(viewer.media.media_id.as_str());
        if let Some(image) = issue
            .images()
            .items()
            .iter()
            .find(|image| image.media.media_id == viewer.media.media_id)
        {
            viewer.comment = image
                .comment_id
                .clone()
                .or_else(|| comment_source(issue, &image.media.media_id));
            if viewer.alt.is_empty() {
                viewer.alt.clone_from(&image.alt_text);
            }
        }
        if viewer.waiting_next && !issue.images().is_loading() {
            let next = issue
                .images()
                .items()
                .iter()
                .position(|image| image.media.media_id == viewer.media.media_id)
                .and_then(|ix| issue.images().items().get(ix + 1));
            if let Some(image) = next {
                viewer.media = image.media.clone();
                viewer.alt.clone_from(&image.alt_text);
                viewer.comment = image
                    .comment_id
                    .clone()
                    .or_else(|| comment_source(issue, &image.media.media_id));
            }
            viewer.waiting_next = false;
        }
    }

    pub(super) fn reveal_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = self.media.viewer.as_ref() else {
            return;
        };
        let id = viewer.media.media_id.clone();
        let issue = self.media.issue.clone();
        if viewer.comment.is_some()
            && let Some(page) = self
                .issue_view
                .snapshot
                .as_ref()
                .and_then(|issue| issue.timeline().page())
        {
            self.issue_view.timeline.show_all_loaded(&page.items);
        }
        self.close_image(window, cx);
        cx.on_next_frame(window, move |_, window, cx| {
            cx.on_next_frame(window, move |this, window, cx| {
                if this.media.issue == issue {
                    if let Some(anchor) = this.media.anchors.get(&id) {
                        anchor.scroll_to(window, cx);
                    }
                    if let Some(focus) = this.media.focus.get(&id) {
                        focus.focus(window, cx);
                    }
                }
            });
        });
    }

    pub(crate) fn read_image_anchor(&mut self) {
        let Some(viewer) = self.media.viewer.as_mut() else {
            return;
        };
        let Some(anchor) = viewer.pending_anchor.as_deref() else {
            return;
        };
        let Some(hub) = &self.hub else {
            return;
        };
        if hub
            .snapshot()
            .issue()
            .is_none_or(|issue| issue.images().is_loading())
        {
            return;
        }
        let _ = hub.load_images(Some(anchor));
        viewer.pending_anchor = None;
    }
}

fn comment_source(issue: &IssueSnapshot, id: &str) -> Option<String> {
    issue
        .timeline()
        .page()?
        .items
        .iter()
        .filter_map(|entry| entry.comment.as_ref())
        .find(|comment| {
            comment.deleted_at.is_none() && comment.media.iter().any(|media| media.media_id == id)
        })
        .map(|comment| comment.id.clone())
}
