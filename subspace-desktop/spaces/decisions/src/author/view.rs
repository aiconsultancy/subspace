//! Native authoring and review dialogs.

use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::decision_writes::DecisionReview;
use subspace_api_client::model::DecisionDetail;
use subspace_framework::dashboard::action::{Intent, action_button, dialog_button};
use subspace_framework::dashboard::dialog::{self, DialogWidth};
use subspace_framework::dashboard::fields;
use subspace_framework::dashboard::{Tokens, feedback};
use subspace_framework::lucide::IconName;

use super::{CONTEXT, CancelDecisionDialog, Editor, Mode, Phase};
use crate::Decisions;

pub(crate) fn render(board: &Decisions, cx: &mut Context<Decisions>) -> Option<AnyElement> {
    let editor = board.author.as_ref()?;
    let tokens = Tokens::current(cx);
    let sending = matches!(editor.phase, Phase::Sending);
    let (title, submit, icon, intent) = words(&editor.mode);
    let content = v_flex()
        .id("decision-author-body")
        .min_h_0()
        .overflow_y_scroll()
        .gap_3()
        .children(progress(board, editor, &tokens, cx))
        .child(match &editor.mode {
            Mode::Draft { draft, .. } => super::form::render(draft, sending, cx),
            Mode::Review { title, review, .. } => review_prompt(title, *review),
        });
    let frame = dialog::frame(&tokens, DialogWidth::D600, title)
        .id("decision-author-dialog")
        .role(Role::Dialog)
        .aria_label(title)
        .test_support()
        .track_focus(&editor.focus)
        .key_context(CONTEXT)
        .on_action(
            cx.listener(|this, _: &CancelDecisionDialog, window, cx| this.close_author(window, cx)),
        )
        .child(content)
        .child(
            dialog::footer()
                .child(
                    dialog_button("decision-author-cancel", Intent::Neutral, None, "Cancel")
                        .disabled(sending)
                        .on_click(cx.listener(|this, _, window, cx| this.close_author(window, cx))),
                )
                .child(
                    dialog_button("decision-author-submit", intent, Some(icon), submit)
                        .disabled(!matches!(editor.phase, Phase::Ready | Phase::Failed(_)))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.submit_author(window, cx)),
                        ),
                ),
        );
    Some(dialog::backdrop(&tokens, frame))
}

fn words(mode: &Mode) -> (&'static str, &'static str, IconName, Intent) {
    match mode {
        Mode::Draft { draft, .. } if draft.hidden_issues => (
            "Edit decision",
            "Save other changes",
            IconName::Check,
            Intent::Primary,
        ),
        Mode::Draft {
            target: Some(_), ..
        } => (
            "Edit decision",
            "Save changes",
            IconName::Check,
            Intent::Primary,
        ),
        Mode::Draft { draft, .. } if draft.predecessor_title.is_some() => (
            "Propose a replacement",
            "Propose replacement",
            IconName::Scale,
            Intent::Primary,
        ),
        Mode::Draft { .. } => (
            "New decision",
            "Propose decision",
            IconName::Scale,
            Intent::Primary,
        ),
        Mode::Review {
            review: DecisionReview::Accept,
            ..
        } => (
            "Accept decision?",
            "Accept decision",
            IconName::Check,
            Intent::Accept,
        ),
        Mode::Review {
            review: DecisionReview::Reject,
            ..
        } => (
            "Reject decision?",
            "Reject decision",
            IconName::X,
            Intent::Reject,
        ),
    }
}

fn review_prompt(title: &str, review: DecisionReview) -> AnyElement {
    let message = match review {
        DecisionReview::Accept => format!(
            "Accept “{title}”? If it replaces an accepted decision, that decision will be marked superseded."
        ),
        DecisionReview::Reject => format!(
            "Reject “{title}”? The decision will stay in the log with its revision history."
        ),
    };
    div().child(message).into_any_element()
}

fn progress(
    board: &Decisions,
    editor: &Editor,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> Option<AnyElement> {
    match &editor.phase {
        Phase::Ready => None,
        Phase::Sending => Some(
            feedback::loading_line(tokens, "decision-author-sending", "Saving decision…")
                .into_any_element(),
        ),
        Phase::Failed(problem) => Some(message(tokens, problem.message())),
        Phase::ReadingCurrent => Some(
            feedback::loading_line(
                tokens,
                "decision-author-reading",
                "Loading the current decision…",
            )
            .into_any_element(),
        ),
        Phase::Current(detail) => Some(current(board, detail, tokens, cx)),
        Phase::ReadFailed => Some(
            v_flex()
                .gap_2()
                .child(message(
                    tokens,
                    "Couldn’t load the current decision. Your draft is still here.",
                ))
                .child(
                    h_flex().child(
                        action_button(
                            "decision-retry-current",
                            Intent::Neutral,
                            Some(IconName::RotateCw),
                            "Retry",
                        )
                        .on_click(cx.listener(|this, _, window, cx| this.read_current(window, cx))),
                    ),
                )
                .into_any_element(),
        ),
        Phase::Unconfirmed => Some(message(
            tokens,
            "The decision may have saved. Close this dialog and check the log before trying again.",
        )),
    }
}

fn current(
    board: &Decisions,
    detail: &DecisionDetail,
    tokens: &Tokens,
    cx: &mut Context<Decisions>,
) -> AnyElement {
    let can_edit = board.can_change(detail);
    let intro = if can_edit {
        "The save was refused because the decision or its replacement changed. Your draft is still here. Review the current record before continuing."
    } else {
        "This decision is no longer editable. You can review the current record and copy your draft."
    };
    let use_version = action_button(
        "decision-use-current",
        Intent::Neutral,
        Some(IconName::RotateCw),
        "Use current version",
    )
    .on_click(cx.listener(|this, _, window, cx| this.use_current(window, cx)));
    let sections = crate::peek::prose(detail)
        .into_iter()
        .map(|(heading, body)| fields::form_field(heading, div().child(body.to_owned())));
    let body = v_flex()
        .gap_2()
        .child(message(tokens, intro))
        .child(div().child(format!(
            "Current revision {} · {}",
            detail.revision_number, detail.title
        )))
        .child(div().child(format!("Decided on {}", detail.decided_on)))
        .children(
            detail
                .supersedes
                .as_ref()
                .map(|prior| div().child(format!("Replaces {}", prior.title))),
        )
        .children(
            (!detail.tags.is_empty())
                .then(|| div().child(format!("Tags · {}", detail.tags.join(", ")))),
        )
        .children((detail.issues.count > 0).then(|| {
            div().child(format!(
                "Linked issues · {}",
                super::draft::issue_identifiers(detail)
            ))
        }))
        .children(sections)
        .children(can_edit.then(|| h_flex().child(use_version)));
    body.into_any_element()
}

fn message(tokens: &Tokens, words: impl Into<gpui_kit::SharedString>) -> AnyElement {
    feedback::empty_line(tokens, "decision-author-message", words).into_any_element()
}
