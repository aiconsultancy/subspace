//! Elements that report the room they were given, so a view can lay out by
//! the width it actually has rather than guessing.
//!
//! The probe is absolutely positioned over its parent and paints nothing, so
//! it changes no layout and takes no hit of its own.

use gpui_kit::{
    Along as _, Axis, Entity, IntoElement, Pixels, SharedString, Styled as _, Window, canvas,
};

/// How far a measurement has to move before it is worth a new frame.
const STEP: f32 = 0.5;
/// A width no layout has reported yet.
pub const UNMEASURED: f32 = 0.;

/// Somewhere to keep a width the layout has not reported yet.
pub fn unmeasured(cx: &mut impl gpui_kit::AppContext) -> Entity<f32> {
    cx.new(|_| UNMEASURED)
}

/// Whether a freshly measured width replaces the one already held.
pub fn changed(held: f32, measured: f32) -> bool {
    (held - measured).abs() > STEP
}

/// Records the width of the element this sits in.
pub fn width(into: Entity<f32>) -> impl IntoElement {
    dimension(into, Axis::Horizontal)
}

/// Records the height of the element this sits in.
pub fn height(into: Entity<f32>) -> impl IntoElement {
    dimension(into, Axis::Vertical)
}

/// The width one line of `text` shapes to in `family` at `size`, for lanes
/// that must fit their widest label.
pub fn text_width(window: &Window, text: &str, family: SharedString, size: Pixels) -> Pixels {
    let mut style = window.text_style();
    style.font_family = family;
    style.font_size = size.into();
    let line: SharedString = text.to_owned().into();
    let run = style.to_run(line.len());
    window
        .text_system()
        .shape_line(line, size, &[run], None)
        .width
}

fn dimension(into: Entity<f32>, axis: Axis) -> impl IntoElement {
    canvas(
        move |bounds, _, cx| {
            let measured = f32::from(bounds.size.along(axis));
            into.update(cx, |width, cx| {
                if changed(*width, measured) {
                    *width = measured;
                    cx.notify();
                }
            });
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::changed;

    #[test]
    fn only_a_real_change_of_width_costs_a_frame() {
        assert!(changed(0., 420.), "the first measurement");
        assert!(changed(420., 300.));
        assert!(!changed(420., 420.));
        assert!(!changed(420., 420.4), "rounding is not a change");
    }
}
