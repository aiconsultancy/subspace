//! Shared frame treatment for dashboard-owned modal dialogs.
//!
//! Callers keep their form state, body layout, handlers, and native GPUI Kit
//! controls. This module only owns the repeated backdrop, frame, title, and
//! footer treatment.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Div, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
};
use gpui_kit::{FontWeight, Styled as _, div, px, relative};

use crate::tokens;
use crate::{MODAL_CONTEXT, Tokens};

const BACKDROP_OPACITY: f32 = 0.6;
const FRAME_MAX_SHARE: f32 = 0.9;
const WIDTH_540: u16 = 540;
const WIDTH_600: u16 = 600;
const WIDTH_780: u16 = 780;

/// Supported widths for dashboard dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogWidth(u16);

impl DialogWidth {
    pub const D540: Self = Self(WIDTH_540);
    pub const D600: Self = Self(WIDTH_600);
    pub const D780: Self = Self(WIDTH_780);
}

/// Dim the dashboard and establish the modal key context around a dialog.
// qual:api - dashboard dialog renderers exercise the native GPUI composition.
pub fn backdrop(tokens: &Tokens, dialog: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .key_context(MODAL_CONTEXT)
        .flex()
        .items_center()
        .justify_center()
        .bg(tokens.bg.opacity(BACKDROP_OPACITY))
        .child(dialog)
        .into_any_element()
}

/// Build the shared dialog frame and heading. Callers append the domain body
/// and a [`footer`] in secondary-to-primary action order.
// qual:api - native GPUI composition; width metadata is tested without asserting layout details.
pub fn frame(tokens: &Tokens, width: DialogWidth, title: impl Into<SharedString>) -> Div {
    v_flex()
        .w(px(f32::from(width.0)))
        .max_w(relative(FRAME_MAX_SHARE))
        .max_h(relative(FRAME_MAX_SHARE))
        .p(tokens::rem(tokens.controls.dialog_pad))
        .gap(tokens::rem(tokens.controls.dialog_gap))
        .rounded(tokens.surface_radius)
        .border_1()
        .border_color(tokens.line)
        .bg(tokens.surface)
        .shadow_lg()
        .child(
            div()
                .text_size(tokens.heading_font)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(title.into()),
        )
}

/// Place dialog actions at the trailing edge. Add secondary actions first and
/// the primary or destructive action last.
// qual:api - dashboard dialog renderers exercise the native GPUI composition.
pub fn footer() -> Div {
    h_flex().w_full().justify_end().gap_2()
}

#[cfg(test)]
mod tests {
    use super::DialogWidth;

    #[test]
    fn supported_dialog_widths_are_stable() {
        assert_eq!(DialogWidth::D540.0, 540);
        assert_eq!(DialogWidth::D600.0, 600);
        assert_eq!(DialogWidth::D780.0, 780);
    }
}
