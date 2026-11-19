use gpui_kit::component::h_flex;
use gpui_kit::{Context, Div, ParentElement as _, Styled as _};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::lucide::IconName;

use super::super::Manager;

impl Manager {
    pub(super) fn rename_actions(cx: &mut Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .child(
                action_button(
                    "web-rename",
                    Intent::Neutral,
                    Some(IconName::Check),
                    "Save name",
                )
                .on_click(cx.listener(|this, _, window, cx| this.rename(window, cx))),
            )
            .child(
                action_button(
                    "web-change-source",
                    Intent::Quiet,
                    Some(IconName::FolderOpen),
                    "Replace source…",
                )
                .on_click(cx.listener(|this, _, _, cx| this.replace(cx))),
            )
    }
}
