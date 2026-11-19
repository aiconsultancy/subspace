//! The Developer page, in debug builds only: Developer mode and how to use
//! the inspector.

use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, Context, IntoElement as _, ParentElement as _, Styled as _};
use subspace_dashboard::Tokens;
use subspace_dashboard::toggle::{switch, toggle_row};

use super::{heading, note, page_frame};
use crate::settings::Settings;

const DESCRIPTION: &str = "Tools for inspecting the interface. Release builds do not include them.";
const DEVELOPER_MODE: &str = "Developer mode";
const DEVELOPER_MODE_HELP: &str = "Shows the Developer menu, its commands and the inspector. Save applies this choice without a restart.";

/// How to open and use the inspector, with this platform's key.
fn inspector_help() -> String {
    format!(
        "Press {} or choose Developer > Inspect elements. Point at an element to see its bounds and component name. Click to pin it and copy its name, ID or source. Press Esc to close the inspector.",
        crate::inspector::shortcut_label()
    )
}

pub(super) fn render(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let body = v_flex()
        .gap_3()
        .child(toggle_row(
            DEVELOPER_MODE,
            switch("settings-developer-mode", cx)
                .checked(settings.developer_mode)
                .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                    settings.set_developer_mode(*enabled, cx);
                })),
        ))
        .child(note(tokens, DEVELOPER_MODE_HELP))
        .child(heading("Inspector", cx))
        .child(note(tokens, inspector_help()));
    page_frame(DESCRIPTION, body, tokens, cx).into_any_element()
}
