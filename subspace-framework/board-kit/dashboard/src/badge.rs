//! Shared geometry for compact count and key-hint badges.

use gpui_kit::{Div, Hsla, ParentElement as _, SharedString, Styled as _, div, px};

mod pill;
pub(crate) use pill::height as pill_height;
pub use pill::{Pill, PillTone};

/// Horizontal padding on each side of a badge.
pub const PADDING_X_PX: f32 = 5.;
/// Badge text size at the default density.
pub const FONT_SIZE_PX: f32 = 11.;
/// The fully rounded compact badge radius.
pub const RADIUS_PX: f32 = 7.;

/// A count pill or keyboard-hint badge with caller-supplied semantic colours.
// qual:api - shared native composition is exercised by shell and menu consumers.
pub fn badge(label: impl Into<SharedString>, background: Hsla, foreground: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .px(px(PADDING_X_PX))
        .rounded(px(RADIUS_PX))
        .bg(background)
        .text_color(foreground)
        .text_size(px(FONT_SIZE_PX))
        .child(label.into())
}
