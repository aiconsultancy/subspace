use gpui_kit::{AppContext as _, Context, PathPromptOptions};

use crate::{
    Attachment, ReportIssue,
    attachments::{IMAGE_LIMIT, ImageSelection, MAX_ATTACHMENTS, read_selection},
    report::Phase,
};

// qual:allow(coupling, oi) reason: "Image I/O methods are a private split of the same ReportIssue owner in report.rs."
impl ReportIssue {
    pub(super) fn choose_image(&mut self, cx: &mut Context<Self>) {
        if self.state.busy() || !self.images_mutable() {
            return;
        }
        if self.state.attachments.is_full() {
            self.state.error = Some(IMAGE_LIMIT.into());
            cx.notify();
            return;
        }
        self.state.image_busy = true;
        let capacity = MAX_ATTACHMENTS - self.state.attachments.len();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Choose PNG or JPEG images".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(paths))) => Some(
                    cx.background_spawn(async move { read_selection(paths, capacity) })
                        .await,
                ),
                Ok(Ok(None)) => None,
                _ => Some(ImageSelection {
                    images: Vec::new(),
                    error: Some("Cannot open the image picker. Try again.".into()),
                }),
            };
            _ = this.update(cx, |this, cx| {
                this.state.image_busy = false;
                if let Some(selection) = result {
                    this.accept_images(selection, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn add_attachment(&mut self, image: Attachment, cx: &mut Context<Self>) {
        match self.state.attachments.append(image) {
            Ok(()) => {
                self.state.invalidate();
                self.state.next_image = 0;
            }
            Err(error) => self.state.error = Some(error),
        }
        cx.notify();
    }

    fn accept_images(&mut self, selection: ImageSelection, cx: &mut Context<Self>) {
        for image in selection.images {
            self.add_attachment(image, cx);
        }
        if let Some(error) = selection.error {
            self.state.error = Some(error);
        }
    }

    pub(super) fn remove_image(&mut self, id: usize, cx: &mut Context<Self>) {
        if self.state.busy() || !self.images_mutable() {
            return;
        }
        if self.state.attachments.remove(id) {
            self.state.invalidate();
            self.state.next_image = 0;
            cx.notify();
        }
    }

    pub(super) fn images_mutable(&self) -> bool {
        matches!(self.state.phase, Phase::Editing | Phase::Review(_))
    }

    pub(super) fn copy_image(&mut self, cx: &mut Context<Self>) {
        if self.state.busy() || self.state.body_pending {
            return;
        }
        let images = self.state.attachments.items();
        let Some(image) = images.get(self.state.next_image) else {
            return;
        };
        let copied = self.state.next_image + 1;
        cx.write_to_clipboard(image.image.clipboard_item());
        self.state.next_image = copied % images.len();
        self.state.notice = Some(format!(
            "Image {copied} of {} sent to the clipboard. Paste it in GitHub before copying the next image. Use Save images… if pasting is unavailable.",
            images.len()
        ));
        cx.notify();
    }

    pub(super) fn save_images(&mut self, cx: &mut Context<Self>) {
        if self.state.busy() || self.state.attachments.len() == 0 {
            return;
        }
        self.state.image_busy = true;
        let images = self.state.attachments.clone();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Save images in this folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => Some(
                        cx.background_spawn(async move { images.save_in(&path) })
                            .await,
                    ),
                    None => None,
                },
                Ok(Ok(None)) => None,
                _ => Some(Err(
                    "Cannot open the Save images folder picker. Try again.".into()
                )),
            };
            _ = this.update(cx, |this, cx| {
                this.state.image_busy = false;
                match result {
                    Some(Ok(())) => {
                        this.state.error = None;
                        this.state.notice = Some(format!(
                            "Saved {} images to the selected folder.",
                            this.state.attachments.len()
                        ))
                    }
                    Some(Err(error)) => this.state.error = Some(error),
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
