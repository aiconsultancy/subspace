use crate::Issues;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::hub::CommentState;
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

pub(crate) fn root(issues: &Issues, tokens: &Tokens, cx: &mut Context<Issues>) -> AnyElement {
    composer(issues, None, tokens, cx)
}

pub(crate) fn thread(
    issues: &Issues,
    comment: &subspace_api_client::model::Comment,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let parent = comment.id.clone();
    let open = issues
        .authoring
        .reply
        .as_ref()
        .is_some_and(|e| e.parent.as_deref() == Some(&parent));
    let mut row = v_flex()
        .id(format!("issue-thread:{}", comment.id))
        .test_support()
        .aria_label(format!("Thread by {}", comment.actual_author.display_name))
        .gap_2();
    if let Some(focus) = issues.authoring.thread_focus.get(&comment.id) {
        row = row.track_focus(focus);
    }
    if open {
        return row
            .child(
                div()
                    .text_color(tokens.muted)
                    .child(format!("Reply to {}", comment.actual_author.display_name)),
            )
            .child(composer(issues, Some(&parent), tokens, cx))
            .into_any_element();
    }
    row = row.child(super::notices::reply_confirmation(
        issues, &parent, tokens, cx,
    ));
    if let Some(draft) = super::held::row(issues, &parent, tokens, cx) {
        return row.child(draft).into_any_element();
    }
    if comment.resolved {
        return row
            .child(
                div()
                    .text_color(tokens.muted)
                    .child("This thread is resolved."),
            )
            .into_any_element();
    }
    if can_reply(issues, comment) {
        row = row.child(
            h_flex().child(
                action_button(
                    format!("issue-reply:{}", comment.id),
                    Intent::Quiet,
                    Some(IconName::Reply),
                    "Reply",
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.focus_comment(Some(parent.clone()), window, cx)
                })),
            ),
        );
    }
    row.into_any_element()
}

fn can_reply(issues: &Issues, comment: &subspace_api_client::model::Comment) -> bool {
    comment.parent_comment_id.is_none()
        && comment.deleted_at.is_none()
        && issues
            .hub
            .as_ref()
            .zip(issues.authoring.issue.as_ref())
            .is_some_and(|(hub, id)| hub.comment_reason(id, Some(&comment.id)).is_none())
}

fn composer(
    issues: &Issues,
    parent: Option<&str>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let Some(editor) = issues.authoring.editor(parent) else {
        return div().into_any_element();
    };
    let Some((hub, issue)) = issues.hub.as_ref().zip(issues.authoring.issue.as_ref()) else {
        return div().into_any_element();
    };
    let draft = hub.comment_draft(issue, parent);
    let reason = hub.comment_reason(issue, parent);
    let mut body = v_flex()
        .id(if parent.is_some() {
            "issue-reply-composer"
        } else {
            "issue-comment-composer"
        })
        .test_support()
        .gap_2()
        .min_w_0();
    if let Some(reason) = reason {
        body = body.child(div().text_color(tokens.muted).child(reason));
        if draft.as_ref().is_none_or(|d| d.body().is_empty()) {
            return body.into_any_element();
        }
    }
    if let Some(draft) = &draft {
        match draft.state() {
            CommentState::Failed(error) => {
                body = body.child(super::notices::failed(&editor.notice_focus, *error, tokens));
            }
            CommentState::Unclear { refresh } => {
                if let Some(reason) = draft.refusal_reason() {
                    body = body.child(div().text_color(tokens.yellow).child(reason));
                }
                body = body.child(super::notices::unknown(
                    issues, parent, *refresh, tokens, cx,
                ));
            }
            _ => {}
        }
        body = body.child(confirmation(draft, tokens, cx));
    }
    body.child(editor.entity.clone()).into_any_element()
}

fn confirmation(
    draft: &subspace_api_client::hub::CommentDraft,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let Some(notice) = draft.notice() else {
        return div().into_any_element();
    };
    v_flex()
        .gap_2()
        .child(
            div()
                .id(if matches!(draft.state(), CommentState::Unclear { .. }) {
                    "issue-comment-check-result"
                } else {
                    "issue-comment-posted"
                })
                .test_support()
                .aria_label(notice)
                .text_color(tokens.muted)
                .child(notice),
        )
        .child(super::notices::refresh_failure(
            draft.refresh_failed() && !matches!(draft.state(), CommentState::Unclear { .. }),
            tokens,
            cx,
        ))
        .into_any_element()
}
