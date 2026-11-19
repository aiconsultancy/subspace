//! The General startup preference, shown only when this app has an update source.

use gpui_kit::{Context, Div};
use subspace_dashboard::toggle::toggle_row;

use super::super::Settings;

pub(super) fn group(settings: &Settings, cx: &mut Context<Settings>) -> Div {
    toggle_row(
        "Automatically check for updates at launch",
        subspace_dashboard::toggle::switch("check-updates-at-startup", cx)
            .checked(settings.check_updates())
            .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                settings.set_check_updates(*enabled, cx);
            })),
    )
}
