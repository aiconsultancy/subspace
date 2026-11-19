//! Shared switches and the layout for independent boolean options.

pub mod track;

use gpui_kit::component::{ActiveTheme as _, h_flex, switch::Switch};
use gpui_kit::{App, Div, ElementId, SharedString, Styled, div, prelude::*};

/// A native switch whose on track meets the switch contrast rule in
/// [`track`]. Build every switch here, not with `Switch::new`: the theme
/// fixes the off track and knob, but only the switch can set its on colour.
pub fn switch(id: impl Into<ElementId>, cx: &App) -> Switch {
    Switch::new(id).color(track::switch_colors(cx.theme()).on)
}

/// Places a wrapping label beside a trailing native switch.
///
/// The caller owns the value and commit boundary. The switch retains its native
/// keyboard, focus, disabled state, and change callback.
pub fn toggle_row(label: impl Into<SharedString>, switch: Switch) -> Div {
    let label = label.into();
    h_flex()
        .w_full()
        .items_start()
        .gap_3()
        .child(div().flex_1().min_w_0().child(label.clone()))
        .child(
            div()
                .flex_shrink_0()
                .child(switch.accessibility_label(label)),
        )
}

#[cfg(test)]
mod tests;
