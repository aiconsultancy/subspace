use gpui_kit::Context;
use subspace_framework::dashboard::action::{Intent, action_button, main_action};
use subspace_framework::dashboard::tokens::ButtonRole;
use subspace_framework::lucide::IconName;

use crate::Issues;

/// New issue…, the Space's main action: the toolbar's one primary button.
pub(crate) fn new_button(cx: &mut Context<Issues>) -> impl gpui_kit::IntoElement {
    main_action("issues-create", IconName::Plus, "New issue…")
        .on_click(cx.listener(|board, _, window, cx| board.open_issue_editor(false, window, cx)))
}

/// Edit issue…, on the issue page's toolbar with the toolbar's geometry.
pub(crate) fn edit_button(cx: &mut Context<Issues>) -> impl gpui_kit::IntoElement {
    action_button(
        "issues-edit",
        Intent::Neutral,
        Some(IconName::Pencil),
        "Edit issue…",
    )
    .role(ButtonRole::Toolbar)
    .on_click(cx.listener(|board, _, window, cx| board.open_issue_editor(true, window, cx)))
}
