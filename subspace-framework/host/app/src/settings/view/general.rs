use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement, Styled,
    TestSupportExt as _,
};
use subspace_dashboard::toggle::toggle_row;
use subspace_dashboard::{Tokens, reveal};

use super::{note, page_frame, pages, updates};
use crate::settings::Settings;
use crate::settings::copy::{help, text};

pub(super) fn render(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let config_file = crate::product::gui_config_file(cx);
    let body = v_flex()
        .gap_3()
        .child(text::CONFIG_FILE)
        .child(
            h_flex()
                .id("settings-config-file")
                .test_support()
                .w_full()
                .gap_3()
                .child(note(tokens, config_file.display().to_string()).flex_1().min_w_0())
                .child(reveal::button("settings-reveal-config", config_file)),
        )
        .child(text::PICK_A_DASHBOARD)
        .child(note(tokens, help::PRIORITY))
        .child(note(tokens, help::RESTART))
        .child(pages::general::confirm_quit_row(settings, tokens, cx))
        .child(toggle_row("Load and refresh Spaces automatically", subspace_dashboard::toggle::switch("automatic-board-refresh", cx).checked(settings.auto_refresh).on_click(cx.listener(|settings, enabled: &bool, _, cx| settings.set_auto_refresh(*enabled, cx)))))
        .child(note(tokens, "Loads Space data after startup and about every 60 seconds. It never prompts for interactive authentication."))
        .children(pages::features::stable_feature_rows(settings, super::super::APP_FEATURES, false, tokens, cx))
        .when(super::super::has_update_source(cx), |panel| panel.child(updates::group(settings, cx)));
    page_frame(
        "Choose app-wide behavior and regular app features.",
        body,
        tokens,
        cx,
    )
    .into_any_element()
}
