use gpui_kit::Context;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::lucide::IconName;

use crate::Issues;

pub(crate) fn new_button(cx: &mut Context<Issues>) -> impl gpui_kit::IntoElement {
    action_button(
        "issues-create",
        Intent::Neutral,
        Some(IconName::Plus),
        "New issue…",
    )
    .on_click(cx.listener(|board, _, window, cx| board.open_issue_editor(false, window, cx)))
}

pub(crate) fn edit_button(cx: &mut Context<Issues>) -> impl gpui_kit::IntoElement {
    action_button(
        "issues-edit",
        Intent::Neutral,
        Some(IconName::Pencil),
        "Edit issue…",
    )
    .on_click(cx.listener(|board, _, window, cx| board.open_issue_editor(true, window, cx)))
}
