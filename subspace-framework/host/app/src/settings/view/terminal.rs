//! The Terminal page: the docked terminal's fonts. Its layout choices are on
//! the Layout page.

use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, Context, IntoElement as _, ParentElement as _, Styled as _};
use subspace_dashboard::Tokens;

use super::{Settings, note, page_frame};

pub(super) fn render(
    settings: &Settings,
    tokens: &Tokens,
    cx: &mut Context<Settings>,
) -> AnyElement {
    let editor = settings
        .terminal
        .as_ref()
        .and_then(|terminal| terminal.fonts.as_ref())
        .map(|fonts| fonts.editor.view());
    let body = v_flex()
        .gap_3()
        .children(editor)
        .child(note(
            tokens,
            "Font choices are kept in the config file's terminal object. Place and size the terminal on the Layout page, or with the buttons in its header.",
        ));
    page_frame(
        "Local shell sessions in the docked terminal, based on libghostty. Sessions and their contents are not restored after the app closes.",
        body,
        tokens,
        cx,
    )
    .into_any_element()
}
