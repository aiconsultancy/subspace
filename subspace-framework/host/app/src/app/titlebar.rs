//! On macOS the toolbar is the window's title bar: the traffic lights sit in
//! its leading edge and dragging its background moves the window. Other
//! platforms keep their native title bar.

use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::{
    App, Div, InteractiveElement as _, MouseButton, Pixels, Point, Stateful, TitlebarOptions,
    Window, point, px,
};

/// AppKit's standard window buttons on macOS 26 and 27: 14 pt squares with
/// 9 pt between them, measured from `NSWindow::standardWindowButton`.
const TRAFFIC_LIGHT_SIZE: f32 = 14.;
const TRAFFIC_LIGHT_GAP: f32 = 9.;
const TRAFFIC_LIGHTS: f32 = 3.;
/// The traffic lights' inset from the window's leading edge.
const TRAFFIC_LIGHTS_X: f32 = 14.;
/// Space after the zoom button before the first control: the wide end of the
/// unified-toolbar spacing in Finder and Xcode.
const CONTENT_GAP: f32 = 20.;
/// The title bar's height before the theme's own is laid out.
const INITIAL_ROW_HEIGHT: f32 = 38.;
const HALF: f32 = 2.;

pub(crate) const INTEGRATED: bool = cfg!(target_os = "macos");

/// The title bar of the product window.
pub(crate) fn options(title: String) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title.into()),
        appears_transparent: INTEGRATED,
        traffic_light_position: INTEGRATED.then(|| traffic_light_origin(px(INITIAL_ROW_HEIGHT))),
    }
}

/// Where the traffic lights go so they share the title-bar row's centre line.
pub(super) fn traffic_light_origin(row_height: Pixels) -> Point<Pixels> {
    let top = (row_height - px(TRAFFIC_LIGHT_SIZE)) / HALF;
    point(px(TRAFFIC_LIGHTS_X), top.max(Pixels::ZERO))
}

/// The zoom button's trailing edge, from the window's leading edge.
pub(super) const TRAFFIC_LIGHTS_END: f32 = TRAFFIC_LIGHTS_X
    + TRAFFIC_LIGHTS * TRAFFIC_LIGHT_SIZE
    + (TRAFFIC_LIGHTS - 1.) * TRAFFIC_LIGHT_GAP;
/// The distance from the window's leading edge to the first title-bar control.
pub(super) const CONTENT_START: f32 = TRAFFIC_LIGHTS_END + CONTENT_GAP;

/// The first row's own leading padding inside a bar already padded by
/// `bar_padding`, clear of the traffic lights except in full screen, where
/// they are hidden.
pub(super) fn leading_inset(window: &Window, bar_padding: Pixels) -> Pixels {
    if INTEGRATED && !window.is_fullscreen() {
        (px(CONTENT_START) - bar_padding).max(Pixels::ZERO)
    } else {
        Pixels::ZERO
    }
}

/// Centres the traffic lights on a title-bar row `row_height` tall. The
/// platform moves its buttons only when the height changes, such as after a
/// theme, density or zoom change.
pub(super) fn align_traffic_lights(
    placed: &mut Option<Pixels>,
    row_height: Pixels,
    window: &Window,
) {
    #[cfg(target_os = "macos")]
    let _ = row_changed(placed, row_height)
        .then(|| window.set_traffic_light_position(traffic_light_origin(row_height)));
    #[cfg(not(target_os = "macos"))]
    let _ = (placed, row_height, window);
}

#[cfg(any(target_os = "macos", test))]
fn row_changed(placed: &mut Option<Pixels>, row_height: Pixels) -> bool {
    INTEGRATED && placed.replace(row_height) != Some(row_height)
}

struct Drag {
    pressed: bool,
}

/// Makes the toolbar background move and zoom the window like a title bar.
/// A press that a control claimed (buttons prevent the default) never drags.
pub(super) fn draggable(bar: Stateful<Div>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    if !INTEGRATED {
        return bar;
    }
    let drag = window.use_keyed_state("titlebar-drag", cx, |_, _| Drag { pressed: false });
    bar.on_double_click(|_, window, _| window.titlebar_double_click())
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(&drag, |drag, _, window, _| {
                drag.pressed = !window.default_prevented();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(&drag, |drag, _, _, _| drag.pressed = false),
        )
        .on_mouse_down_out(window.listener_for(&drag, |drag, _, _, _| drag.pressed = false))
        .on_mouse_move(window.listener_for(&drag, |drag, _, window, _| {
            if drag.pressed {
                drag.pressed = false;
                window.start_window_move();
            }
        }))
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::{TRAFFIC_LIGHT_SIZE, TRAFFIC_LIGHTS_X, row_changed, traffic_light_origin};

    #[test]
    fn the_traffic_lights_share_the_rows_centre_line() {
        for height in [30., 38., 44., 52.] {
            let origin = traffic_light_origin(px(height));
            assert_eq!(origin.y + px(TRAFFIC_LIGHT_SIZE / 2.), px(height / 2.));
            assert_eq!(origin.x, px(TRAFFIC_LIGHTS_X));
        }
    }

    #[test]
    fn the_lights_move_only_when_the_row_height_changes() {
        let mut placed = None;
        assert_eq!(row_changed(&mut placed, px(38.)), super::INTEGRATED);
        assert!(!row_changed(&mut placed, px(38.)));
        assert_eq!(row_changed(&mut placed, px(44.)), super::INTEGRATED);
    }
}
