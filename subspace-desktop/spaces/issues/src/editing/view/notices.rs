use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

use super::super::{IssueEditor, Phase, TrackerConfirmation};
use crate::Issues;

pub(super) fn notice(editor: &IssueEditor, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    if let Phase::ConfirmingChildren(confirmation) = editor.phase {
        return tracker_confirmation(confirmation, tokens, cx);
    }
    if editor.discard {
        return discard(editor, cx);
    }
    if let Phase::Conflict(latest) = &editor.phase {
        return conflict(latest.as_deref(), tokens, cx);
    }
    if editor.elsewhere.is_some() && matches!(editor.phase, Phase::Editing) {
        return elsewhere(tokens, cx);
    }
    let message: Option<SharedString> = match editor.phase {
        Phase::Failed(subspace_api_client::issue_writes::IssueSaveError::Summary) => Some(editor.summary_error().into()),
        Phase::Failed(error) => Some(error.to_string().into()),
        Phase::SeveralMatches => Some(subspace_api_client::issue_writes::IssueSaveError::SeveralMatches.to_string().into()),
        Phase::Unconfirmed => {
            Some("The save may have completed. Check before trying again.".into())
        }
        Phase::Checked => {
            Some("No new matching issue was found. Create again may create a duplicate.".into())
        }
        Phase::CheckFailed => Some("Couldn’t check for the issue. The create may have completed. Check again before creating another.".into()),
        Phase::Checking => Some("Checking for a new issue…".into()),
        Phase::LoadingConflict => Some("Reading the latest issue…".into()),
        _ => None,
    };
    let row = div().when_some(message, |row, message| message_row(row, message, tokens));
    row.when(
        matches!(
            editor.phase,
            Phase::Unconfirmed | Phase::Checked | Phase::CheckFailed | Phase::SeveralMatches
        ),
        |row| {
            row.child(
                action_button(
                    "issue-editor-check",
                    Intent::Neutral,
                    Some(IconName::RefreshCw),
                    "Check issue",
                )
                .on_click(cx.listener(|board, _, window, cx| board.check_issue_save(window, cx))),
            )
        },
    )
}

fn tracker_confirmation(
    confirmation: TrackerConfirmation,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let (message, yes, no) = match confirmation {
        TrackerConfirmation::Assignment => {
            ("Assign the unassigned child issues as well?", "Yes", "No")
        }
        TrackerConfirmation::State { .. } => (
            "Move the open child issues to this state?",
            "Move children",
            "Cancel",
        ),
    };
    v_flex()
        .gap_2()
        .child(message_row(div(), message.into(), tokens))
        .child(
            h_flex().flex_wrap().gap_2().children(
                [
                    ("issue-tracker-children-yes", yes, Intent::Primary, true),
                    ("issue-tracker-children-no", no, Intent::Neutral, false),
                ]
                .into_iter()
                .map(|(id, text, intent, answer)| {
                    action_button(id, intent, None, text)
                        .compact()
                        .on_click(cx.listener(move |board, _, window, cx| {
                            board.answer_tracker_confirmation(answer, window, cx);
                        }))
                }),
            ),
        )
}

fn message_row(row: Div, message: SharedString, tokens: &Tokens) -> Div {
    row.child(
        div()
            .id("issue-editor-notice")
            .test_support()
            .aria_label(message.clone())
            .text_color(tokens.fg_strong)
            .text_sm()
            .child(message),
    )
}

/// Someone changed the issue while this draft was open.
fn elsewhere(tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    h_flex()
        .gap_2()
        .child(message_row(div(), "Changed elsewhere.".into(), tokens))
        .child(
            action_button(
                "issue-editor-reload-elsewhere",
                Intent::Neutral,
                Some(IconName::RefreshCw),
                "Reload",
            )
            .on_click(cx.listener(|board, _, window, cx| board.reload_changed_issue(window, cx))),
        )
}

fn conflict(
    issue: Option<&subspace_api_client::model::IssueDetail>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let result = v_flex().gap_2().child(message_row(
        div(),
        "The issue changed while you were editing. Your draft is still here.".into(),
        tokens,
    ));
    match issue {
        Some(issue) => result
            .child(format!("Current title: {}", issue.fields.title))
            .child(
                action_button(
                    "issue-editor-use-version",
                    Intent::Neutral,
                    Some(IconName::RefreshCw),
                    "Use latest version and keep edits",
                )
                .on_click(cx.listener(|board, _, window, cx| board.keep_issue_edits(window, cx))),
            ),
        None => result.child(
            action_button(
                "issue-editor-reload-version",
                Intent::Neutral,
                Some(IconName::RefreshCw),
                "Read latest issue",
            )
            .on_click(cx.listener(|board, _, window, cx| board.read_issue_conflict(window, cx))),
        ),
    }
}

fn discard(editor: &IssueEditor, cx: &mut Context<Issues>) -> Div {
    v_flex()
        .gap_2()
        .child(
            div()
                .id("issue-editor-discard")
                .test_support()
                .child("Discard this issue draft?"),
        )
        .when(editor.frozen(), |row| {
            row.child("The save may have completed. Discard removes only this draft.")
        })
        .child(
            h_flex()
                .gap_2()
                .child(
                    action_button(
                        "issue-editor-keep",
                        Intent::Neutral,
                        Some(IconName::Pencil),
                        "Keep editing",
                    )
                    .on_click(cx.listener(|board, _, _, cx| {
                        if let Some(editor) = &mut board.editor {
                            editor.discard = false;
                        }
                        cx.notify();
                    })),
                )
                .child(
                    action_button(
                        "issue-editor-discard-now",
                        Intent::Reject,
                        Some(IconName::Trash),
                        "Discard",
                    )
                    .on_click(
                        cx.listener(|board, _, window, cx| board.close_issue_editor(window, cx)),
                    ),
                ),
        )
}
