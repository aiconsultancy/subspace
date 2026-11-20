//! Standalone attachments being added: each shows its state, with Retry or
//! Check while that can still change the outcome, and Dismiss once it failed.
use crate::Issues;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, div, prelude::FluentBuilder as _,
};
use subspace_api_client::hub::{AttachmentState, AttachmentUpload};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};

/// One upload: its name, what is happening to it, and what can still be
/// done about it.
pub(super) fn row(upload: &AttachmentUpload, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let (status, failed) = status(upload.state());
    let id = upload.id().to_owned();
    h_flex()
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
        )
        // Retry or Check only where it can still change the outcome.
        .when(failed && upload.retryable(), |row| {
            row.child(retry_button(&id, upload.checks(), cx))
        })
        .when(failed, |row| row.child(dismiss_button(&id, cx)))
}

/// What the row says, and whether the upload failed, which offers Dismiss.
fn status(state: &AttachmentState) -> (String, bool) {
    match state {
        AttachmentState::Failed(error) => (error.message(), true),
        AttachmentState::NotFound => ("The upload was not stored. Choose Retry.".into(), true),
        AttachmentState::Checking => ("Checking…".into(), false),
        _ => ("Uploading…".into(), false),
    }
}

/// Check completes a transfer whose bytes may be stored; Retry sends again.
fn retry_button(id: &str, checking: bool, cx: &Context<Issues>) -> impl IntoElement {
    let retry = id.to_owned();
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
    }))
}

fn dismiss_button(id: &str, cx: &Context<Issues>) -> impl IntoElement {
    let dismiss = id.to_owned();
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
    }))
}
