//! The image view of a diff: the before and after blobs of one image file.

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Div, Image, ImageFormat, InteractiveElement as _,
    IntoElement as _, ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, img,
    px,
};
use subspace_framework::client_github::model::GhFileStatus;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, feedback};

use super::{BYTES_PER_KB, DiffView, IMAGE_GAP_PX, IMAGE_MAX_PX, IMAGE_NOTE_W_PX, IMAGE_PAD_PX};

pub(super) enum Slot {
    Hidden,
    Loading,
    Ready(Arc<Image>, usize),
    Failed,
}

pub(super) struct ImageView {
    pub(super) file: usize,
    before: Slot,
    after: Slot,
}

/// Which blob a side of the image view shows: the base or the head sha, and
/// the path the file had there.
type SideSource = Option<(String, String)>;

impl DiffView {
    pub(super) fn open_image(&mut self, file: usize, cx: &mut Context<Self>) {
        let Some(view) = self.files.get(file) else {
            return;
        };
        if self.image.as_ref().is_some_and(|image| image.file == file) {
            return;
        }
        let status = view.file.status;
        let before_path = view
            .file
            .previous_path
            .clone()
            .unwrap_or_else(|| view.file.path.clone());
        let after_path = view.file.path.clone();
        let before = (status != GhFileStatus::Added).then(|| (self.base_sha.clone(), before_path));
        let after = (status != GhFileStatus::Removed).then(|| (self.head_sha.clone(), after_path));
        self.image = Some(ImageView {
            file,
            before: slot_for(&before),
            after: slot_for(&after),
        });
        self.load_side(file, false, before, cx);
        self.load_side(file, true, after, cx);
        cx.notify();
    }

    /// Fetch one side's blob and put it in its slot when it arrives.
    fn load_side(&self, file: usize, right: bool, source: SideSource, cx: &mut Context<Self>) {
        let (Some((sha, path)), Some((client, login, repo))) = (source, self.github_connection())
        else {
            return;
        };
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { client.image(&login, &repo, &sha, &path) })
                .await;
            _ = view.update(cx, |view, cx| {
                let Some(image) = view.image.as_mut().filter(|image| image.file == file) else {
                    return;
                };
                let slot = match result {
                    Ok(bytes) => match ImageFormat::from_mime_type(bytes.content_type) {
                        Some(format) => {
                            let size = bytes.bytes.len();
                            Slot::Ready(Arc::new(Image::from_bytes(format, bytes.bytes)), size)
                        }
                        None => Slot::Failed,
                    },
                    Err(_) => Slot::Failed,
                };
                if right {
                    image.after = slot;
                } else {
                    image.before = slot;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn image_view(
        &self,
        image: &ImageView,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(view) = self.files.get(image.file) else {
            return div().into_any_element();
        };
        v_flex()
            .id("diff-image")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .p(px(IMAGE_PAD_PX))
            .gap_4()
            .text_size(tokens.row_font)
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .text_color(tokens.fg_strong)
                            .child(view.file.path.clone()),
                    )
                    .child(
                        action_button(
                            "diff-image-close",
                            Intent::Quiet,
                            Some(IconName::X),
                            "Close (Esc)",
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.image = None;
                            cx.notify();
                        })),
                    ),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap(px(IMAGE_GAP_PX))
                    .children(side("Before", &image.before, tokens))
                    .children(side("After", &image.after, tokens)),
            )
            .into_any_element()
    }
}

/// A side starts loading when it has a blob to show.
fn slot_for(source: &SideSource) -> Slot {
    if source.is_some() {
        Slot::Loading
    } else {
        Slot::Hidden
    }
}

/// One labelled side of the image view, or nothing when the file has no
/// such side.
fn side(label: &'static str, slot: &Slot, tokens: &Tokens) -> Option<Div> {
    let body = match slot {
        Slot::Hidden => return None,
        Slot::Loading => {
            feedback::loading_line(tokens, "github-image-loading", "Loading…").into_any_element()
        }
        Slot::Failed => div()
            .max_w(px(IMAGE_NOTE_W_PX))
            .text_color(tokens.muted)
            .child("Cannot show this image. It may be larger than 10 MB or stored in Git LFS.")
            .into_any_element(),
        Slot::Ready(image, size) => ready_image(image, *size, tokens),
    };
    Some(
        v_flex()
            .gap_2()
            .child(div().text_color(tokens.fg_strong).child(label))
            .child(body),
    )
}

/// The loaded image and its size in kilobytes.
fn ready_image(image: &Arc<Image>, size: usize, tokens: &Tokens) -> AnyElement {
    v_flex()
        .gap_1()
        .child(
            img(image.clone())
                .max_w(px(IMAGE_MAX_PX))
                .max_h(px(IMAGE_MAX_PX))
                .bg(tokens.pane)
                .border_1()
                .border_color(tokens.line),
        )
        .child(
            div()
                .text_xs()
                .text_color(tokens.muted)
                .child(format!("{:.1} KB", size as f32 / BYTES_PER_KB)),
        )
        .into_any_element()
}
