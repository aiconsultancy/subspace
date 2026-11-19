//! Closed reply editors retain a visible route to their memory-only draft.
use crate::Issues;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div};
use subspace_api_client::hub::CommentState;
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

pub(super) fn row(
    issues: &Issues,
    parent: &str,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let (hub, issue) = issues.hub.as_ref().zip(issues.authoring.issue.as_ref())?;
    let draft = hub.comment_draft(issue, Some(parent))?;
    if draft.body().is_empty() && !draft.is_frozen() && !draft.has_attachments() {
        return None;
    }
    let mut row = header(parent, tokens, cx);
    if let Some(reason) = hub.comment_reason(issue, Some(parent)) {
        row = row.child(div().text_color(tokens.muted).child(reason));
    }
    match draft.state() {
        CommentState::Unclear { refresh } => {
            row = row.child(super::notices::unknown(
                issues,
                Some(parent),
                *refresh,
                tokens,
                cx,
            ))
        }
        CommentState::Posting => {
            row = row.child(div().text_color(tokens.muted).child("Posting reply…"))
        }
        state => {
            let mut actions = h_flex().gap_2();
            if let CommentState::Failed(error) = state {
                row = row.child(div().text_color(tokens.red).child(error.to_string()));
                if hub.comment_reason(issue, Some(parent)).is_none() {
                    actions = actions.child(retry(parent, cx));
                }
            }
            row = row.child(actions.child(super::notices::discard_button(
                Some(parent.into()),
                false,
                cx,
            )));
        }
    }
    Some(row.into_any_element())
}

fn header(parent: &str, tokens: &Tokens, cx: &mut Context<Issues>) -> gpui_kit::Div {
    let thread = parent.to_owned();
    v_flex().gap_2().child(
        h_flex()
            .gap_2()
            .child(div().text_color(tokens.muted).child("Reply draft"))
            .child(
                action_button(
                    format!("issue-draft-open:{parent}"),
                    Intent::Quiet,
                    Some(IconName::Pencil),
                    "Open",
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus_comment(Some(thread.clone()), window, cx)
                })),
            ),
    )
}

fn retry(parent: &str, cx: &mut Context<Issues>) -> impl IntoElement {
    let parent = parent.to_owned();
    action_button(
        "issue-comment-retry",
        Intent::Neutral,
        Some(IconName::RotateCw),
        "Retry",
    )
    .on_click(cx.listener(move |this, _, _, cx| this.post_comment(Some(&parent), true, cx)))
}
