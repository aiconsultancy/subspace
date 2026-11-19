//! Presentation of the retained close confirmation.
use super::{CONTEXT, CancelClose, ConfirmClose, cancel, submit};
use crate::Github;
use gpui_kit::component::{Disableable as _, FocusTrapElement as _};
use gpui_kit::{AnyElement, Context, InteractiveElement as _, ParentElement as _};
use subspace_framework::dashboard::action::{Intent, dialog_button};
use subspace_framework::dashboard::dialog::{self, DialogWidth};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, fields};

pub(crate) fn render(board: &Github, cx: &mut Context<Github>) -> Option<AnyElement> {
    let pending = board.close_confirmation.as_ref()?;
    let tokens = Tokens::current(cx);
    let frame = dialog::frame(
        &tokens,
        DialogWidth::D600,
        format!("Close pull request {}?", pending.item.key),
    )
    .key_context(CONTEXT)
    .focus_trap("github-close-dialog", &pending.focus)
    .on_action(cx.listener(|board, _: &CancelClose, window, cx| cancel(board, window, cx)))
    .on_action(cx.listener(|board, _: &ConfirmClose, window, cx| submit(board, window, cx)))
    .child(fields::form_field(
        "Comment (optional)",
        fields::textarea(&pending.comment, "Comment (optional)")
            .disabled(board.writing)
            .readonly(pending.posted),
    ))
    .child(fields::note(
        &tokens,
        if board.writing {
            "Closing pull request…"
        } else {
            "Cmd/Ctrl+Enter: close pull request · Esc to cancel"
        },
    ))
    .children(
        pending
            .error
            .as_ref()
            .map(|error| fields::field_error(&tokens, "github-close-error", error)),
    )
    .child(footer(board.writing, cx));
    Some(dialog::backdrop(&tokens, frame))
}

fn footer(busy: bool, cx: &mut Context<Github>) -> impl gpui_kit::IntoElement {
    dialog::footer()
        .child(
            dialog_button(
                "github-close-cancel",
                Intent::Neutral,
                Some(IconName::X),
                "Cancel",
            )
            .disabled(busy)
            .on_click(cx.listener(|board, _, window, cx| cancel(board, window, cx))),
        )
        .child(
            dialog_button(
                "github-close-submit",
                Intent::Reject,
                Some(IconName::GitPullRequestClosed),
                "Close pull request",
            )
            .disabled(busy)
            .on_click(cx.listener(|board, _, window, cx| submit(board, window, cx))),
        )
}
