//! Presentation of the merge action and its confirmation.

use gpui_kit::component::{Disableable as _, FocusTrapElement as _};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
};
use subspace_framework::client_github::model::{GhItem, MergeMethod, PrState};
use subspace_framework::dashboard::action::{Intent, action_button, dialog_button};
use subspace_framework::dashboard::dialog::{self, DialogWidth};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, fields};

use super::{CONTEXT, CancelMerge, cancel, open, still_current, submit, target};
use crate::{Github, close_pull_request::is_closable};

pub(crate) fn button(
    board: &Github,
    item: &GhItem,
    primary: bool,
    cx: &mut Context<Github>,
) -> Option<AnyElement> {
    if !is_closable(item) {
        return None;
    }
    let available = item.state == PrState::Open && target(board, &item.key).is_some();
    let tooltip = match (item.state, available) {
        (PrState::Draft, _) => "Mark this pull request ready for review before merging.",
        (_, true) => "Merge pull request…",
        _ => "GitHub has not confirmed that this pull request can be merged.",
    };
    let key = item.key.clone();
    let intent = if available && primary {
        Intent::Primary
    } else {
        Intent::Neutral
    };
    Some(
        action_button("gh-merge", intent, Some(IconName::GitPullRequest), "Merge…")
            .accessibility_label("Merge pull request…")
            .tooltip(tooltip)
            .disabled(board.writing || !available)
            .on_click(cx.listener(move |board, _, window, cx| open(board, &key, window, cx)))
            .into_any_element(),
    )
}

pub(crate) fn render(board: &Github, cx: &mut Context<Github>) -> Option<AnyElement> {
    let pending = board.merge_confirmation.as_ref()?;
    let tokens = Tokens::current(cx);
    let current = still_current(board, pending);
    let method = match pending.target.method {
        MergeMethod::Merge => "Create a merge commit",
        MergeMethod::Squash => "Squash the commits",
        MergeMethod::Rebase => "Rebase the commits",
    };
    let instruction = pending.item.base_ref.as_deref().map_or_else(
        || format!("{method}."),
        |base| format!("{method} into {base}."),
    );
    let frame = dialog::frame(&tokens, DialogWidth::D600, format!("Merge pull request {}?", pending.item.key))
        .key_context(CONTEXT)
        .focus_trap("github-merge-dialog", &pending.focus)
        .on_action(cx.listener(|board, _: &CancelMerge, window, cx| cancel(board, window, cx)))
        .child(fields::note(&tokens, instruction))
        .children(if board.writing {
            Some(fields::note(&tokens, "Merging pull request…"))
        } else {
            pending.error.or((!current).then_some(
                "This pull request changed or is no longer ready to merge. Close this dialog and review its current state.",
            )).map(|text| fields::field_error(&tokens, "github-merge-error", text))
        })
        .child(dialog::footer()
            .child(dialog_button("github-merge-cancel", Intent::Neutral, None, "Cancel")
                .disabled(board.writing)
                .on_click(cx.listener(|board, _, window, cx| cancel(board, window, cx))))
            .child(dialog_button("github-merge-submit", Intent::Primary, Some(IconName::GitPullRequest), "Merge pull request")
                .disabled(board.writing || !current)
                .on_click(cx.listener(|board, _, window, cx| submit(board, window, cx)))));
    Some(dialog::backdrop(&tokens, frame))
}
