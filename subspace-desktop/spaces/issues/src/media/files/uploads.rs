//! Standalone attachments being added: each shows its state, with Retry or
//! Check while that can still change the outcome, and Dismiss once it failed.
//! When checks in a row cannot tell whether NETT stored a file, the person
//! may choose to send it again, warned that it may then be attached twice.
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
    let (status, failed) = status(upload);
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
        .when(upload.may_send_again(), |row| {
            row.child(send_again_button(&id, cx))
        })
        .when(failed, |row| row.child(dismiss_button(&id, cx)))
}

/// What the row says, and whether the upload failed, which offers Dismiss.
fn status(upload: &AttachmentUpload) -> (String, bool) {
    match upload.state() {
        AttachmentState::Failed(_) if upload.may_send_again() => (
            "NETT could not tell whether this file was stored. Check again, or send it again \
             only if it is missing: it may then be attached twice."
                .into(),
            true,
        ),
        AttachmentState::Failed(_) | AttachmentState::NotFound
            if upload.released() && !upload.checks() =>
        {
            (
                "The upload was not stored. Choose the file again.".into(),
                true,
            )
        }
        AttachmentState::Failed(error) => (error.message(), true),
        AttachmentState::NotFound => ("The upload was not stored. Choose Retry.".into(), true),
        AttachmentState::Checking => ("Checking…".into(), false),
        _ => ("Uploading…".into(), false),
    }
}

/// Sends the file again under a new key, by the person's choice.
fn send_again_button(id: &str, cx: &Context<Issues>) -> impl IntoElement {
    let again = id.to_owned();
    action_button(
        format!("attachment-upload-send-again:{id}"),
        Intent::Neutral,
        None,
        "Send again",
    )
    .small()
    .on_click(cx.listener(move |this, _, _, _| {
        if let Some(hub) = &this.hub {
            hub.send_attachment_upload_again(&again);
        }
    }))
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

#[cfg(test)]
mod tests {
    use super::status;
    use subspace_api_client::hub::{AttachmentState, AttachmentUpload};
    use subspace_api_client::uploads::UploadError;

    fn upload(
        state: AttachmentState,
        checks: bool,
        unknown: u8,
        released: bool,
    ) -> AttachmentUpload {
        AttachmentUpload::for_test("notes.txt", state, checks, unknown, released).unwrap()
    }

    #[test]
    fn checks_that_cannot_tell_offer_sending_again_with_a_warning() {
        let unknown = AttachmentState::Failed(UploadError::Checking);
        let offered = upload(unknown.clone(), true, 3, false);
        assert!(offered.may_send_again());
        let (text, failed) = status(&offered);
        assert!(failed && text.contains("attached twice"), "{text}");
        // Before then, only Check.
        let early = upload(unknown, true, 2, false);
        assert!(!early.may_send_again());
        assert!(!status(&early).0.contains("attached twice"));
    }

    #[test]
    fn an_upload_whose_bytes_went_asks_for_the_file_again() {
        let absent = upload(AttachmentState::NotFound, false, 0, true);
        assert!(!absent.retryable());
        assert_eq!(
            status(&absent),
            (
                "The upload was not stored. Choose the file again.".into(),
                true
            )
        );
    }
}
