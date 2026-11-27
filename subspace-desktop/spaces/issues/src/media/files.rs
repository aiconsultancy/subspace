//! File metadata remains readable without opening an external download URL.
//! A text or Markdown file opens in the attachment panel beside the page.
//! The panel adds and saves files, and removes files the service lets go.
mod disk;
mod uploads;

use crate::Issues;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Div, InteractiveElement as _, IntoElement,
    ParentElement as _, Styled as _, TestSupportExt as _, div, prelude::FluentBuilder as _,
};
use subspace_api_client::model::IssueMedia;
use subspace_api_client::{
    hub::{FilesSnapshot, IssueSnapshot},
    uploads::UploadError,
};
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
    let actions = standalone
        && issues
            .hub
            .as_ref()
            .is_some_and(|hub| files.accepts_files(hub.attachment_protocol()));
    let rows = listed(files, standalone);
    if rows.is_empty() && !files.is_failed() && !actions && files.uploads().is_empty() {
        return None;
    }
    let tokens = Tokens::current(cx);
    let mut panel = v_flex().gap_3().child(crate::view::section_heading(
        "Attachments",
        IconName::Paperclip,
        &tokens,
    ));
    for media in rows {
        let held = issues
            .hub
            .as_ref()
            .is_some_and(|hub| hub.comment_holds_attachment(media));
        panel = panel.child(row(
            media,
            files.removing(&media.media_id),
            actions,
            held,
            cx,
        ));
    }
    for upload in files.uploads() {
        panel = panel.child(uploads::row(upload, &tokens, cx));
    }
    let panel = panel
        .children(
            issues
                .attachment_error
                .clone()
                .map(|error| message("attachment-error", error, &tokens)),
        )
        .children(
            files
                .notice()
                .map(|notice| message("attachment-notice", notice, &tokens)),
        )
        .when(actions, |panel| panel.child(add_button(cx)))
        .when(files.is_failed(), |panel| {
            panel
                .child("Couldn’t load attachments")
                .child(retry_button(cx))
        });
    Some(panel.into_any_element())
}

/// The files the panel lists: standalone attachments of every kind, a GIF
/// included, where NETT serves them; otherwise only files, as the images
/// panel shows the images.
fn listed(files: &FilesSnapshot, standalone: bool) -> Vec<&IssueMedia> {
    files
        .items()
        .iter()
        .filter(|media| media.kind == "file" || (standalone && media.is_standalone()))
        .collect()
}

/// A line of muted text under the rows: why a file was not added, or what
/// happened to the last change.
fn message(
    id: impl Into<gpui_kit::ElementId>,
    text: impl IntoElement,
    tokens: &Tokens,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .text_color(tokens.muted)
        .child(text)
}

fn add_button(cx: &Context<Issues>) -> impl IntoElement {
    action_button(
        "attachment-add",
        Intent::Neutral,
        Some(IconName::Plus),
        "Add file",
    )
    .on_click(cx.listener(|this, _, _, cx| this.pick_attachment(cx)))
}

fn retry_button(cx: &Context<Issues>) -> impl IntoElement {
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
    }))
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
    actions: bool,
    held: bool,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let label = label(media);
    let shown = if super::text::viewable(media) {
        open_button(media.clone(), label, "attachment-open", cx)
    } else {
        subspace_framework::dashboard::media::file_chip(label, &tokens).into_any_element()
    };
    if !(actions && media.can_save_attachment()) {
        return shown;
    }
    let layout = h_flex()
        .min_w_0()
        .gap_2()
        .child(div().flex_1().min_w_0().child(shown))
        .child(save_button(media, cx))
        .when(!held && media.can_remove_attachment(), |row| {
            row.child(remove_button(media.clone(), removing, cx))
        });
    let reason = held
        .then_some("Used by your unposted comment.")
        .or_else(|| media.attachment_removal_notice());
    if let Some(reason) = reason {
        return v_flex()
            .gap_1()
            .child(layout)
            .child(message(
                format!("attachment-removal-notice:{}", media.media_id),
                reason,
                &tokens,
            ))
            .into_any_element();
    }
    layout.into_any_element()
}

fn save_button(media: &IssueMedia, cx: &Context<Issues>) -> impl IntoElement {
    let media = media.clone();
    action_button(
        format!("attachment-save:{}", media.media_id),
        Intent::Quiet,
        Some(IconName::Download),
        "Save",
    )
    .small()
    .on_click(cx.listener(move |this, _, _, cx| this.save_attachment(media.clone(), cx)))
}

fn remove_button(media: IssueMedia, removing: bool, cx: &Context<Issues>) -> impl IntoElement {
    let remove = action_button(
        format!("attachment-remove:{}", media.media_id),
        Intent::Quiet,
        Some(IconName::Trash),
        "Remove",
    )
    .small()
    .on_click(cx.listener(move |this, _, _, _| {
        if let Some(hub) = &this.hub
            && !hub.comment_holds_attachment(&media)
        {
            hub.remove_attachment(&media.media_id);
        }
    }));
    if removing {
        ActionState::Busy.apply(remove)
    } else {
        remove
    }
}

// qual:allow(coupling, oi) reason: "The panel's commands coordinate the board's hub, the open issue's scope and its attachment error line."
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
            let protocol = hub.attachment_protocol();
            let read = cx
                .background_spawn(async move { disk::read_chosen(&path, protocol) })
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
        read: Result<(String, String, Vec<u8>), UploadError>,
    ) {
        let (name, mime, bytes) = match read {
            Ok(file) => file,
            Err(error) => {
                self.attachment_error = Some(error.message());
                return;
            }
        };
        let current = self.hub.as_ref().is_some_and(|open| open.same(hub));
        // Only a changed issue, opening, scope or hub says so; a file the
        // client refuses keeps its own reason.
        self.attachment_error = if current {
            hub.add_attachment_to(target, &name, &mime, bytes)
                .err()
                .map(|error| match error {
                    UploadError::Removed => CHANGED.to_owned(),
                    UploadError::Budget => UPLOADS_FULL.to_owned(),
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

    /// Saves a listed attachment's bytes where the person chooses.
    pub(crate) fn save_attachment(&mut self, media: IssueMedia, cx: &mut Context<Self>) {
        let (Some(scope), Some(revision), Some(hub)) =
            (self.media.scope, self.media.revision, self.hub.clone())
        else {
            return;
        };
        let picker = cx.prompt_for_new_path(
            &disk::save_folder(),
            Some(&disk::suggested_name(&media.file_name)),
        );
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
