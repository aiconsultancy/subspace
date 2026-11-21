//! Standalone attachments being added: each shows its state, with Retry or
//! Check while that can still change the outcome, and Dismiss once it failed.
//! When checks in a row cannot tell whether NETT stored a file, the person
//! may choose to send it again, warned that it may then be attached twice.
use crate::Issues;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, div, prelude::FluentBuilder as _,
};
use subspace_api_client::hub::{AttachmentState, AttachmentUpload};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

/// One upload: its name, what is happening to it, and what can still be
/// done about it.
pub(super) fn row(
    upload: &AttachmentUpload,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> impl IntoElement + use<> {
    let (status, failed) = status(upload);
    let id = upload.id();
    let mut actions = Vec::new();
    // Retry or Check only where it can still change the outcome.
    if failed && upload.retryable() {
        actions.push(retry_button(id, upload.checks(), cx).into_any_element());
    }
    if upload.may_send_again() {
        actions.push(send_again_button(id, cx).into_any_element());
    }
    if failed {
        actions.push(dismiss_button(id, cx).into_any_element());
    }
    layout(upload, status, actions, tokens)
}

/// An upload row within the panel's column: the name on its own line, cut
/// with an ellipsis when it does not fit, as listed attachments are; what is
/// happening to it below, wrapping however long it is; then its buttons,
/// wrapping onto another line when they do not fit.
fn layout(
    upload: &AttachmentUpload,
    status: String,
    actions: Vec<AnyElement>,
    tokens: &Tokens,
) -> impl IntoElement + use<> {
    let id = upload.id();
    v_flex()
        .id(format!("attachment-upload:{id}"))
        .test_support()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                .id(format!("attachment-upload-name:{id}"))
                .test_support()
                .w_full()
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
                        .child(upload.file_name().to_owned()),
                ),
        )
        .child(
            div()
                .id(format!("attachment-upload-status:{id}"))
                .test_support()
                .w_full()
                .text_color(tokens.muted)
                .child(status),
        )
        .when(!actions.is_empty(), |row| {
            row.child(
                h_flex()
                    .id(format!("attachment-upload-actions:{id}"))
                    .test_support()
                    .w_full()
                    .flex_wrap()
                    .gap_2()
                    .children(actions),
            )
        })
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
    use super::{layout, status};
    use gpui_kit::{
        AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _,
        Pixels, Render, Styled as _, TestAppContext, TestSupportExt as _, VisualTestContext,
        Window, WindowBounds, WindowOptions, div, px, size,
    };
    use subspace_api_client::hub::{AttachmentState, AttachmentUpload};
    use subspace_api_client::uploads::UploadError;
    use subspace_framework::dashboard::{
        Tokens,
        action::{Intent, action_button},
    };

    /// A narrow main column, as a small window gives the issue page.
    const COLUMN: f32 = 360.;
    /// The least width the name keeps: well over a few characters.
    const NAME_MIN: f32 = 200.;

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

    /// An upload row in a column `COLUMN` wide, with every button an upload
    /// row can show.
    struct Column(AttachmentUpload);

    impl Render for Column {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let tokens = Tokens::current(cx);
            let (text, _) = status(&self.0);
            let actions = ["Check", "Send again", "Dismiss"]
                .into_iter()
                .map(|label| {
                    action_button(format!("fixture-{label}"), Intent::Neutral, None, label)
                        .into_any_element()
                })
                .collect();
            div().p_4().child(
                div()
                    .id("fixture-column")
                    .test_support()
                    .w(px(COLUMN))
                    .child(layout(&self.0, text, actions, &tokens)),
            )
        }
    }

    /// The bounds of the element `id` in the window.
    fn bounds(cx: &mut VisualTestContext, id: String) -> Bounds<Pixels> {
        cx.update(|window, _| {
            use gpui_kit::test::TestWindowExt as _;
            window.find(id).bounds()
        })
    }

    /// However long the status, it wraps within the column, the name keeps
    /// one line of at least `NAME_MIN`, and the buttons stay inside.
    #[gpui_kit::test]
    fn a_long_status_wraps_within_the_column_and_the_name_keeps_its_width(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let name = "fixture-checklist-for-the-attachments-panel-review.txt";
        let unknown = AttachmentState::Failed(UploadError::Checking);
        let rows = [
            // The longest status: Send again, with its warning.
            AttachmentUpload::for_test(name, unknown.clone(), true, 3, false).unwrap(),
            // The upload may have completed.
            AttachmentUpload::for_test(name, unknown, true, 0, false).unwrap(),
        ];
        for upload in rows {
            let id = upload.id().to_owned();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(COLUMN + 100.), px(600.)),
                ))),
                ..Default::default()
            };
            let (window, _) = cx
                .update(|cx| gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Column(upload))))
                .unwrap();
            let shown = VisualTestContext::from_window(window, cx).into_mut();
            shown.update(|window, cx| {
                use gpui_kit::test::TestWindowExt as _;
                window.render_frame(cx);
            });
            let column = bounds(shown, "fixture-column".into());
            let row = bounds(shown, format!("attachment-upload:{id}"));
            let name = bounds(shown, format!("attachment-upload-name:{id}"));
            let text = bounds(shown, format!("attachment-upload-status:{id}"));
            let actions = bounds(shown, format!("attachment-upload-actions:{id}"));
            for part in [row, name, text, actions] {
                assert!(
                    part.left() >= column.left() && part.right() <= column.right(),
                    "{part:?} stays in {column:?}"
                );
            }
            assert!(name.size.width >= px(NAME_MIN), "{name:?}");
            // One line for the name, however long; the status wraps below.
            assert!(name.size.height < text.size.height, "{name:?} {text:?}");
            assert!(name.bottom() <= text.top() && text.bottom() <= actions.top());
        }
    }
}
