//! File metadata remains readable without opening an external download URL.
//! A text or Markdown file opens in the attachment panel beside the page.
//! Where NETT serves standalone attachments for the organisation, the panel
//! also adds, saves and removes them.
use crate::Issues;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Div, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::hub::{AttachmentState, AttachmentUpload, IssueSnapshot};
use subspace_api_client::model::IssueMedia;
use subspace_framework::dashboard::{
    Tokens,
    action::{ActionState, Intent, action_button, card_button},
};
use subspace_framework::lucide::IconName;

/// The largest standalone attachment saved from the panel.
const MAX_SAVED_BYTES: usize = 25_000_000;
/// Why a chosen file was not added.
pub(crate) const CHANGED: &str =
    "The issue changed while the file was chosen, so nothing was added. Choose the file again.";
/// Why a chosen file was not added while other uploads hold their bytes.
pub(crate) const UPLOADS_FULL: &str = "Keep at most eight uploads and 32 MiB waiting on an issue, and 64 MiB across drafts and uploads. Dismiss one, then choose the file again.";

#[cfg(test)]
thread_local! {
    static STANDALONE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Whether the organisation serves standalone attachments. Tests can say.
pub(crate) fn standalone_attachments() -> bool {
    #[cfg(test)]
    if let Some(served) = STANDALONE.with(std::cell::Cell::get) {
        return served;
    }
    subspace_api_client::capabilities().standalone_attachments
}

/// Makes the panel behave as if the organisation did, or did not, serve
/// standalone attachments.
#[cfg(test)]
pub(crate) fn serve_standalone_for_test(served: Option<bool>) {
    STANDALONE.with(|cell| cell.set(served));
}

pub(crate) fn panel(
    issues: &Issues,
    open: &IssueSnapshot,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let files = open.files();
    let standalone = standalone_attachments();
    // Standalone attachments of every kind, a GIF included, are listed;
    // otherwise the images panel shows the images.
    let rows = files
        .items()
        .iter()
        .filter(|media| media.kind == "file" || (standalone && media.is_standalone()))
        .collect::<Vec<_>>();
    if rows.is_empty() && !files.is_failed() && !standalone {
        return None;
    }
    let tokens = Tokens::current(cx);
    let mut panel = v_flex().gap_3().child(crate::view::section_heading(
        "Attachments",
        IconName::Paperclip,
        &tokens,
    ));
    for media in rows {
        panel = panel.child(row(
            media,
            standalone && files.removing(&media.media_id),
            standalone,
            cx,
        ));
    }
    for upload in files.uploads() {
        panel = panel.child(upload_row(upload, &tokens, cx));
    }
    if let Some(error) = issues.attachment_error.clone() {
        panel = panel.child(
            div()
                .id("attachment-error")
                .test_support()
                .text_color(tokens.muted)
                .child(error),
        );
    }
    if let Some(notice) = files.notice() {
        panel = panel.child(
            div()
                .id("attachment-notice")
                .test_support()
                .text_color(tokens.muted)
                .child(notice),
        );
    }
    if standalone {
        panel = panel.child(
            action_button(
                "attachment-add",
                Intent::Neutral,
                Some(IconName::Plus),
                "Add file",
            )
            .on_click(cx.listener(|this, _, _, cx| this.pick_attachment(cx))),
        );
    }
    if files.is_failed() {
        panel = panel.child("Couldn’t load attachments").child(
            action_button(
                "retry-issue-attachments",
                Intent::Neutral,
                Some(IconName::RefreshCw),
                "Retry",
            )
            .on_click(cx.listener(|this, _, _, _| {
                if let Some(hub) = &this.hub {
                    _ = hub.load_attachments();
                }
            })),
        );
    }
    Some(panel.into_any_element())
}

fn label(media: &IssueMedia) -> String {
    format!(
        "{} ({}, {})",
        media.file_name,
        media.content_type,
        crate::markdown::format_file_size(media.size_bytes.max(0) as u64)
    )
}

fn row(
    media: &IssueMedia,
    removing: bool,
    standalone: bool,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let label = label(media);
    let shown = if super::text::viewable(media) {
        open_button(media.clone(), label, "attachment-open", cx)
    } else {
        subspace_framework::dashboard::media::file_chip(label, &tokens).into_any_element()
    };
    if !(standalone && media.is_standalone()) {
        return shown;
    }
    let id = media.media_id.clone();
    let save = {
        let media = media.clone();
        action_button(
            format!("attachment-save:{id}"),
            Intent::Quiet,
            Some(IconName::Download),
            "Save",
        )
        .small()
        .on_click(cx.listener(move |this, _, _, cx| this.save_attachment(media.clone(), cx)))
    };
    let remove = action_button(
        format!("attachment-remove:{id}"),
        Intent::Quiet,
        Some(IconName::Trash),
        "Remove",
    )
    .small()
    .on_click(cx.listener(move |this, _, _, _| {
        if let Some(hub) = &this.hub {
            hub.remove_attachment(&id);
        }
    }));
    let remove = if removing {
        ActionState::Busy.apply(remove)
    } else {
        remove
    };
    h_flex()
        .min_w_0()
        .gap_2()
        .child(div().flex_1().min_w_0().child(shown))
        .child(save)
        .child(remove)
        .into_any_element()
}

fn upload_row(upload: &AttachmentUpload, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let (status, action) = match upload.state() {
        AttachmentState::Failed(error) => (error.message(), true),
        AttachmentState::NotFound => ("The upload was not stored. Choose Retry.".into(), true),
        AttachmentState::Checking => ("Checking…".into(), false),
        _ => ("Uploading…".into(), false),
    };
    // Retry or Check only where it can still change the outcome.
    let retryable = upload.retryable();
    let id = upload.id().to_owned();
    let mut row = h_flex()
        .min_w_0()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(upload.file_name().to_owned()),
        )
        .child(
            div()
                .id(format!("attachment-upload-status:{id}"))
                .test_support()
                .text_color(tokens.muted)
                .child(status),
        );
    if action {
        let retry = id.clone();
        let dismiss = id.clone();
        let checking = upload.checks();
        if retryable {
            row = row.child(
                action_button(
                    format!("attachment-upload-retry:{id}"),
                    Intent::Neutral,
                    None,
                    if checking { "Check" } else { "Retry" },
                )
                .small()
                .on_click(cx.listener(move |this, _, _, _| {
                    if let Some(hub) = &this.hub {
                        hub.retry_attachment_upload(&retry);
                    }
                })),
            );
        }
        row = row.child(
            action_button(
                format!("attachment-upload-dismiss:{id}"),
                Intent::Quiet,
                None,
                "Dismiss",
            )
            .small()
            .on_click(cx.listener(move |this, _, _, _| {
                if let Some(hub) = &this.hub {
                    hub.dismiss_attachment_upload(&dismiss);
                }
            })),
        );
    }
    row
}

impl Issues {
    /// Chooses a file to add as a standalone attachment, to the issue open
    /// now, in the hub and scope open now.
    fn pick_attachment(&mut self, cx: &mut Context<Self>) {
        let Some((hub, target)) = self
            .hub
            .clone()
            .and_then(|hub| hub.attachment_target().map(|target| (hub, target)))
        else {
            return;
        };
        let picker = cx.prompt_for_paths(gpui_kit::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let read = cx
                .background_spawn(async move {
                    let mime = subspace_api_client::uploads::file_mime(&path)?;
                    let name = path.file_name()?.to_string_lossy().into_owned();
                    let bytes = std::fs::read(&path).ok()?;
                    Some((name, mime.to_owned(), bytes))
                })
                .await;
            _ = this.update(cx, |this, cx| {
                this.attach_picked(&hub, &target, read);
                cx.notify();
            });
        })
        .detach();
    }

    /// Adds a chosen file to the issue it was chosen for. Nothing is added
    /// when the person moved to another issue, organisation or account
    /// meanwhile.
    pub(crate) fn attach_picked(
        &mut self,
        hub: &subspace_api_client::hub::Hub,
        target: &subspace_api_client::hub::AttachmentTarget,
        read: Option<(String, String, Vec<u8>)>,
    ) {
        let Some((name, mime, bytes)) = read else {
            self.attachment_error = Some("Choose a supported file of up to 10 MB.".into());
            return;
        };
        let current = self.hub.as_ref().is_some_and(|open| open.same(hub));
        // Only a changed issue, opening, scope or hub says so; a file the
        // client refuses keeps its own reason.
        self.attachment_error = if current {
            hub.add_attachment_to(target, &name, &mime, bytes)
                .err()
                .map(|error| match error {
                    subspace_api_client::uploads::UploadError::Removed => CHANGED.to_owned(),
                    subspace_api_client::uploads::UploadError::Budget => UPLOADS_FULL.to_owned(),
                    error => error.message(),
                })
        } else {
            Some(CHANGED.to_owned())
        };
    }

    /// Adds a file as a standalone attachment on the open issue.
    #[cfg(test)]
    pub(crate) fn add_attachment(&mut self, name: &str, mime: &str, bytes: Vec<u8>) {
        let Some(hub) = &self.hub else {
            return;
        };
        self.attachment_error = hub
            .add_attachment(name, mime, bytes)
            .err()
            .map(|error| error.message());
    }

    /// Saves a standalone attachment's bytes, read through its file grant,
    /// where the person chooses.
    pub(crate) fn save_attachment(&mut self, media: IssueMedia, cx: &mut Context<Self>) {
        let (Some(scope), Some(revision), Some(hub)) =
            (self.media.scope, self.media.revision, self.hub.clone())
        else {
            return;
        };
        let picker = cx.prompt_for_new_path(std::path::Path::new("."), Some(&media.file_name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picker.await else {
                return;
            };
            let result = match hub
                .fetch_file(scope, revision, media, MAX_SAVED_BYTES)
                .await
            {
                Ok(bytes) => cx
                    .background_spawn(async move { std::fs::write(&path, bytes) })
                    .await
                    .map_err(|_| "Couldn't save the file there.".to_owned()),
                Err(error) => Err(error.to_string()),
            };
            _ = this.update(cx, |this, cx| {
                this.attachment_error = result.err();
                cx.notify();
            });
        })
        .detach();
    }
}

/// The whole file row opens the attachment panel, with the chip's words. `place`
/// keeps the Attachments row and an inline reference to the same file apart.
pub(super) fn open_button(
    media: IssueMedia,
    label: String,
    place: &str,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let id = format!("{place}:{}", media.media_id);
    card_button(id, format!("Open {label}"), open_label(&label, &tokens))
        .min_w_0()
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.open_text(media.clone(), window, cx);
        }))
        .into_any_element()
}

fn open_label(label: &str, tokens: &Tokens) -> Div {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .child(
            Icon::from(IconName::FileText)
                .small()
                .flex_shrink_0()
                .text_color(tokens.muted),
        )
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(label.to_owned()),
        )
}
