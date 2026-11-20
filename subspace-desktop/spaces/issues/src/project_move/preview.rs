//! The retained move preview: what the move changes, each issue's number
//! before and after, and Move to confirm it.
use super::Issues;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
    prelude::FluentBuilder as _,
};
use subspace_api_client::issue_moves::{MoveItem, MovePreview};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

/// The preview of a move, ready to confirm.
pub(super) fn render(preview: &MovePreview, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    v_flex()
        .gap_3()
        .flex_1()
        .min_h_0()
        .child(question(preview))
        .child(numbering(preview))
        .child(consequences(preview))
        .when(refuses_archived(preview), |col| {
            col.child(div().id("issue-move-archived").test_support().child(
                subspace_api_client::issue_writes::IssueSaveError::ArchivedSubIssue.to_string(),
            ))
        })
        .child(identifiers(preview, tokens))
        .child(
            h_flex().justify_end().child(
                action_button(
                    "issue-move-confirm",
                    Intent::Primary,
                    Some(IconName::FolderInput),
                    "Move",
                )
                .disabled(refuses_archived(preview))
                .on_click(cx.listener(|this, _, window, cx| this.confirm_project_move(window, cx))),
            ),
        )
}

/// What the person is asked to confirm.
fn question(preview: &MovePreview) -> String {
    format!(
        "Move {} {} to {} · {}?",
        preview.issues.len(),
        if preview.issues.len() == 1 {
            "issue"
        } else {
            "issues"
        },
        preview.destination.key,
        preview.destination.name
    )
}

/// Every moved issue: its number now and in the destination.
fn identifiers(preview: &MovePreview, tokens: &Tokens) -> impl IntoElement {
    v_flex()
        .id("issue-move-identifiers")
        .test_support()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .gap_1()
        .children(
            preview
                .issues
                .iter()
                .map(|item| identifier_row(item, tokens)),
        )
}

fn identifier_row(item: &MoveItem, tokens: &Tokens) -> Div {
    h_flex()
        .gap_4()
        .py_1()
        .child(div().w_40().child(item.old_identifier.clone()))
        .child(
            div().w_40().child(
                item.new_identifier
                    .clone()
                    .unwrap_or_else(|| "New number on move".into()),
            ),
        )
        .child(div().text_color(tokens.muted).child(format!(
            "{}{}",
            item.state_name,
            if item.archived { " · Archived" } else { "" }
        )))
}

/// How the moved issues are numbered: as previewed, or, where the service
/// numbers them on confirmation, in the destination when the move completes.
fn numbering(preview: &MovePreview) -> String {
    if preview
        .issues
        .iter()
        .any(|item| item.new_identifier.is_none())
    {
        format!(
            "Each issue gets a new {} number when the move completes. The exact changes are shown below.",
            preview.destination.key
        )
    } else {
        "Each issue will receive a new project key and number. The exact changes are shown below."
            .into()
    }
}

/// NETT refuses to move an archived sub-issue with its parent; Subspace API
/// moves it and keeps it archived.
fn refuses_archived(preview: &MovePreview) -> bool {
    subspace_api_client::backend() == subspace_api_client::Backend::Nett
        && preview.moves_archived_sub_issue()
}

/// What the move clears and what happens to sub-issues. Subspace API's
/// preview keeps its fixed wording; NETT's follows the rows.
fn consequences(preview: &MovePreview) -> String {
    if subspace_api_client::backend() != subspace_api_client::Backend::Nett {
        return "Cycle, milestone and estimate values will be cleared. A separately moved child will leave its parent. History, files and old issue links will be kept.".into();
    }
    let issues = &preview.issues;
    let cleared: Vec<&str> = [
        (issues.iter().any(|i| i.cycle_cleared), "cycle"),
        (issues.iter().any(|i| i.milestone_cleared), "milestone"),
        (issues.iter().any(|i| i.estimate_cleared), "estimate"),
    ]
    .into_iter()
    .filter_map(|(cleared, name)| cleared.then_some(name))
    .collect();
    let mut text = match cleared.as_slice() {
        [] => String::new(),
        [one] => format!("{} values will be cleared. ", capitalised(one)),
        [first, second] => format!(
            "{} and {second} values will be cleared. ",
            capitalised(first)
        ),
        _ => "Cycle, milestone and estimate values will be cleared. ".into(),
    };
    text.push_str(if issues.iter().any(|i| i.parent_detached) {
        "A separately moved child will leave its parent. "
    } else if issues.len() > 1 {
        "Sub-issues move with their parent. "
    } else {
        ""
    });
    text.push_str("History, files and old issue links will be kept.");
    text
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
