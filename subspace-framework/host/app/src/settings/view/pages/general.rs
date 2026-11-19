//! App-wide switches on the General page that are not Settings features.

use gpui_kit::component::v_flex;
use gpui_kit::{Context, Div, ParentElement as _, Styled as _};
use subspace_dashboard::Tokens;
use subspace_dashboard::toggle::toggle_row;

use super::super::{Settings, help, note};

/// The "Confirm before quitting" switch (REQ-34), applied on Save.
pub(in crate::settings::view) fn confirm_quit_row(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> Div {
    v_flex()
        .gap_1()
        .child(toggle_row(
            "Confirm before quitting",
            subspace_dashboard::toggle::switch("confirm-before-quitting", cx)
                .checked(settings.confirm_quit)
                .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                    settings.set_confirm_quit(*enabled, cx);
                })),
        ))
        .child(note(tokens, help::CONFIRM_QUIT))
}
