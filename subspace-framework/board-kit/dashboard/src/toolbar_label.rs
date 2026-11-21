//! A toolbar fact with nothing to choose, such as the only signed-in account.
//! It sits on the toolbar band like a button but is plain text: no hover,
//! focus or caret promises an action.

use gpui_kit::accesskit::Role;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
};

use crate::Tokens;
use crate::icon_button::Metrics;
use crate::tokens::{ButtonRole, rem};

/// What a toolbar label shows and says.
pub struct ToolbarLabel {
    /// A muted decorative glyph before the text, or none for plain quiet
    /// text such as "Grouped by state".
    pub icon: Option<Icon>,
    pub text: SharedString,
    /// The spoken name, which gives the text its context, such as
    /// "Signed in to GitHub as octo-dev".
    pub accessible: SharedString,
}

/// `label` in the Toolbar role's height, padding and text size, with its
/// muted decorative glyph when it has one, so it shares the band's centre
/// line with its buttons.
pub fn label<I: Into<ElementId>>(
    id: I,
    label: ToolbarLabel,
    cx: &App,
) -> impl IntoElement + use<I> {
    let tokens = Tokens::current(cx);
    let metrics = tokens.controls.button(ButtonRole::Toolbar);
    let glyph = Metrics::current(cx).glyph();
    h_flex()
        .id(id)
        .test_support()
        .role(Role::Label)
        .aria_label(label.accessible)
        .flex_shrink_0()
        .h(rem(metrics.height))
        .px(rem(metrics.pad_x))
        .gap_1()
        .text_size(rem(metrics.font))
        .text_color(tokens.muted)
        .children(
            label
                .icon
                .map(|icon| icon.size(glyph).text_color(tokens.muted)),
        )
        .child(label.text)
}
