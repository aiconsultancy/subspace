use crate::Issues;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

pub(super) fn failed(
    focus: &gpui_kit::FocusHandle,
    error: subspace_api_client::WriteError,
    tokens: &Tokens,
) -> impl IntoElement {
    let copy = match error {
        subspace_api_client::WriteError::SignInNeeded => crate::scope::SIGN_IN_MESSAGE.to_owned(),
        _ => error.to_string(),
    };
    div()
        .id("issue-comment-failed")
        .test_support()
        .track_focus(focus)
        .aria_label(copy.clone())
        .text_color(tokens.red)
        .child(copy)
}

pub(super) fn unknown(
    issues: &Issues,
    parent: Option<&str>,
    refresh: bool,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let thread = parent.map(str::to_owned);
    let warning = issues.authoring.discarding.as_ref() == Some(&thread);
    let mut notice = v_flex().id("issue-comment-unknown").test_support().gap_2();
    if let Some(editor) = issues.authoring.editor(parent) {
        notice = notice.track_focus(&editor.notice_focus);
    }
    let copy = if warning {
        "This comment may already be posted."
    } else {
        "We couldn't confirm whether your comment was posted. Check discussion before posting again."
    };
    notice = notice
        .aria_label(copy)
        .child(div().text_color(tokens.yellow).child(copy));
    notice
        .child(unknown_actions(issues, thread, warning, refresh, cx))
        .into_any_element()
}

fn unknown_actions(
    issues: &Issues,
    thread: Option<String>,
    warning: bool,
    refresh: bool,
    cx: &mut Context<Issues>,
) -> gpui_kit::Div {
    let draft = issues
        .hub
        .as_ref()
        .zip(issues.authoring.issue.as_ref())
        .and_then(|(hub, issue)| hub.comment_draft(issue, thread.as_deref()));
    let checking = draft.as_ref().is_some_and(|draft| draft.checking());
    let ready = draft.as_ref().is_some_and(|draft| draft.is_retry_ready());
    let mut actions =
        h_flex()
            .gap_2()
            .flex_wrap()
            .child(check_button(thread.clone(), checking, cx));
    if refresh {
        actions = actions
            .child(div().child("Posting again may post this comment twice."))
            .child(post_again(thread.clone(), ready, cx));
    }
    actions = actions.child(discard_button(thread, warning, cx));
    if warning {
        actions = actions.child(
            action_button(
                "issue-comment-keep",
                Intent::Quiet,
                Some(IconName::Undo2),
                "Keep draft",
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.authoring.discarding = None;
                cx.notify();
            })),
        );
    }
    actions
}

fn check_button(
    parent: Option<String>,
    checking: bool,
    cx: &mut Context<Issues>,
) -> impl IntoElement {
    action_button(
        "issue-comment-check",
        Intent::Neutral,
        Some(IconName::RefreshCw),
        if checking {
            "Checking discussion…"
        } else {
            "Check discussion"
        },
    )
    .disabled(checking)
    .on_click(cx.listener(move |this, _, _, cx| {
        if let (Some(hub), Some(issue)) = (&this.hub, &this.authoring.issue) {
            _ = hub.check_comment(issue, parent.as_deref());
        }
        this.pull_comment_snapshot(cx);
    }))
}

fn post_again(parent: Option<String>, ready: bool, cx: &mut Context<Issues>) -> impl IntoElement {
    action_button(
        "issue-comment-post-again",
        Intent::Neutral,
        Some(IconName::Send),
        "Post again",
    )
    .disabled(!ready)
    .on_click(cx.listener(move |this, _, _, cx| this.post_comment(parent.as_deref(), true, cx)))
}

pub(super) fn discard_button(
    thread: Option<String>,
    warning: bool,
    cx: &mut Context<Issues>,
) -> impl IntoElement {
    action_button(
        "issue-comment-discard",
        Intent::Neutral,
        Some(IconName::X),
        if warning { "Discard draft" } else { "Discard" },
    )
    .on_click(cx.listener(move |this, _, window, cx| {
        let unclear = this
            .hub
            .as_ref()
            .zip(this.authoring.issue.as_ref())
            .and_then(|(h, i)| h.comment_draft(i, thread.as_deref()))
            .is_some_and(|d| {
                matches!(
                    d.state(),
                    subspace_api_client::hub::CommentState::Unclear { .. }
                )
            });
        if warning || !unclear {
            if let (Some(h), Some(i)) = (&this.hub, &this.authoring.issue) {
                h.discard_comment(i, thread.as_deref());
            }
            this.authoring.discarding = None;
            this.pull_comment_snapshot(cx);
            this.focus_comment(thread.clone(), window, cx);
        } else {
            this.authoring.discarding = Some(thread.clone());
            cx.notify();
        }
    }))
}

pub(super) fn refresh_failure(
    failed: bool,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    if !failed {
        return div().into_any_element();
    }
    let notice = v_flex()
        .id("issue-comment-refresh-failed")
        .test_support()
        .gap_2();
    notice
        .child(
            div()
                .text_color(tokens.yellow)
                .child("Posted. Couldn't refresh discussion."),
        )
        .child(
            action_button(
                "issue-comment-refresh",
                Intent::Neutral,
                Some(IconName::RefreshCw),
                "Refresh discussion",
            )
            .on_click(cx.listener(|this, _, _, _| {
                if let (Some(hub), Some(issue)) = (&this.hub, &this.authoring.issue) {
                    _ = hub.refresh_discussion(issue);
                }
            })),
        )
        .into_any_element()
}

pub(super) fn reply_confirmation(
    issues: &Issues,
    parent: &str,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> impl IntoElement {
    let mut row = v_flex().gap_2();
    if let Some((notice, failed)) = issues
        .hub
        .as_ref()
        .zip(issues.authoring.issue.as_ref())
        .and_then(|(h, i)| h.comment_draft(i, Some(parent)))
        .filter(|draft| {
            !matches!(
                draft.state(),
                subspace_api_client::hub::CommentState::Unclear { .. }
            )
        })
        .and_then(|d| {
            d.notice()
                .map(|notice| (notice.to_owned(), d.refresh_failed()))
        })
    {
        row = row
            .child(div().text_color(tokens.muted).child(notice))
            .child(refresh_failure(failed, tokens, cx));
    }
    row
}
