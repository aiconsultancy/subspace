use crate::Issues;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{AnyElement, Context, Div, IntoElement, ParentElement as _, Styled as _, div};
use subspace_api_client::hub::{Attachment, AttachmentState, CommentDraft};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

pub(in crate::authoring) fn rows(
    issues: &Issues,
    parent: Option<&str>,
    draft: Option<&CommentDraft>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let mut rows = v_flex().gap_2().min_w_0();
    if let Some(error) = &issues.authoring.intake_error {
        rows = rows.child(div().text_color(tokens.yellow).child(error.clone()));
    }
    let frozen = draft.is_some_and(|d| d.is_frozen());
    if let Some(draft) = draft {
        for image in draft.attachments() {
            rows = rows.child(
                AttachmentRow {
                    image,
                    parent,
                    frozen,
                }
                .render(cx),
            );
        }
    }
    rows.child(attach(issues, parent, frozen, cx))
        .into_any_element()
}

struct AttachmentRow<'a> {
    image: &'a Attachment,
    parent: Option<&'a str>,
    frozen: bool,
}

impl AttachmentRow<'_> {
    fn render(&self, cx: &mut Context<Issues>) -> Div {
        let state = self.image.state();
        let mut row = h_flex()
            .gap_2()
            .min_w_0()
            .child(div().flex_1().child(format!(
                "{} · {}",
                self.image.file_name(),
                self.label(&state)
            )));
        if matches!(state, AttachmentState::Failed(_)) {
            row = row.child(self.retry(&state, cx));
        }
        row.child(self.remove(cx))
    }
    fn label(&self, state: &AttachmentState) -> String {
        match state {
            AttachmentState::Preparing => "Preparing".into(),
            AttachmentState::Uploading => format!(
                "Uploading · {} / {} bytes",
                self.image.transferred_bytes(),
                self.image.size_bytes()
            ),
            AttachmentState::Checking => "Checking".into(),
            AttachmentState::NotFound => "No upload found. Choose Remove to continue.".into(),
            AttachmentState::Ready => "Ready".into(),
            AttachmentState::Failed(error) => error.message(),
        }
    }
    fn retry(&self, state: &AttachmentState, cx: &mut Context<Issues>) -> impl IntoElement {
        let id = self.image.id();
        let parent = self.parent.map(str::to_owned);
        let label = if matches!(
            state,
            AttachmentState::Failed(
                subspace_api_client::uploads::UploadError::Checking
                    | subspace_api_client::uploads::UploadError::Unclear { .. }
            )
        ) {
            "Check again"
        } else {
            "Retry"
        };
        action_button(
            format!("retry-image:{id}"),
            Intent::Quiet,
            Some(IconName::RefreshCw),
            label,
        )
        .disabled(self.frozen || !self.image.retry_ready())
        .on_click(cx.listener(move |this, _, _, _| {
            if let Some((hub, issue)) = this.hub.as_ref().zip(this.authoring.issue.as_ref()) {
                hub.retry_comment_image(issue, parent.as_deref(), &id);
            }
        }))
    }
    fn remove(&self, cx: &mut Context<Issues>) -> impl IntoElement {
        let id = self.image.id();
        let parent = self.parent.map(str::to_owned);
        action_button(
            format!("remove-image:{id}"),
            Intent::Quiet,
            Some(IconName::X),
            "Remove",
        )
        .disabled(self.frozen)
        .on_click(cx.listener(move |this, _, _, _| {
            if let Some((hub, issue)) = this.hub.as_ref().zip(this.authoring.issue.as_ref()) {
                hub.remove_comment_image(issue, parent.as_deref(), &id);
            }
        }))
    }
}

fn attach(
    issues: &Issues,
    parent: Option<&str>,
    frozen: bool,
    cx: &mut Context<Issues>,
) -> impl IntoElement {
    let permitted = issues
        .hub
        .as_ref()
        .zip(issues.authoring.issue.as_ref())
        .is_some_and(|(hub, issue)| hub.comment_reason(issue, parent).is_none());
    let parent = parent.map(str::to_owned);
    h_flex().child(
        action_button(
            "attach-comment-image",
            Intent::Quiet,
            Some(IconName::Paperclip),
            "Attach files…",
        )
        .disabled(frozen || !permitted || issues.authoring.intake_busy)
        .on_click(
            cx.listener(move |this, _, _, cx| this.attach_comment_images(parent.clone(), cx)),
        ),
    )
}
