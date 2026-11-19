//! Pointer and rendered-cap regressions for the production navigation split.

mod fixture;
mod sizing;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    App, AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, TestAppContext, Window, WindowHandle, point, px,
};
use subspace_dashboard::density::Density;
use subspace_dashboard::prefs;

use super::{COLLAPSED_KEY, SubspaceApp, WIDTH_KEY, collapsed_rail_width, read_width_preference};
use fixture::{MutableBadge, open_fixture};

const EDGE_INSET: f32 = 4.;
const OUTWARD_MARGIN: f32 = 30.;
const MID_DRAG_END: u32 = 4;
const DRAG_STEPS: u32 = 8;

#[gpui_kit::test]
fn pointer_drag_uses_the_drawn_resizable_handle_and_persists_only_on_release(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let window = open_fixture(cx, root.path(), Some("999999999999"), Some("180"));
    draw(&window, cx);
    draw(&window, cx);
    let (collapse_start, collapse_end) = collapse_pointer(&window, cx);
    window
        .update(cx, |app, _, cx| {
            assert!(
                app.live_rail_width(cx).is_some_and(|width| width < 180.),
                "real pointer move must resize the live panel; live={:?}",
                app.live_rail_width(cx)
            );
            assert_eq!(prefs::get(cx, WIDTH_KEY).as_deref(), Some("180"));
        })
        .unwrap();
    let cap = cap(&window, cx);
    finish_collapse(&window, cx, collapse_start, collapse_end);
    let outward_end = outward_pointer(&window, cx, cap);
    window
        .update(cx, |app, _, cx| {
            assert!(app.rail_shows_labels(cx), "outward drag must reveal labels");
            assert_eq!(read_width_preference(cx), Some(180.));
            assert!(
                app.live_rail_width(cx)
                    .is_some_and(|width| width > collapsed_rail_width())
            );
        })
        .unwrap();
    release_pointer(&window, cx, outward_end);
    window
        .update(cx, |app, _, cx| {
            assert!(!app.navigation_collapsed);
            let width = app.live_rail_width(cx).unwrap_or_default();
            let stored = read_width_preference(cx).unwrap_or_default();
            assert!(
                (width - cap).abs() < 1.,
                "outward drag must clamp at cap: {width} vs {cap}"
            );
            assert!(
                (stored - cap).abs() < 1.,
                "mouse-up stores the capped width: {stored} vs {cap}"
            );
            assert!(
                (width - 100.).abs() > 1.,
                "resize must not fall back to the 100px floor"
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn rendered_badge_and_font_changes_clamp_then_restore_the_preferred_width(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open_fixture(cx, root.path(), Some("999999999999"), Some("180"));
    draw(&window, cx);
    let wide_cap = cap(&window, cx);
    assert!(
        wide_cap > 180.,
        "badge must widen the measured cap: {wide_cap}"
    );
    assert_eq!(live_width(&window, cx), 180.);

    cx.update(|cx| {
        cx.global_mut::<MutableBadge>().0.replace(None);
        cx.set_global(Density::from_json(r#"{"--row-font":"10px"}"#));
        crate::theme::sync_font_sizes(cx);
    });
    draw(&window, cx);
    draw(&window, cx);
    let narrow_cap = cap(&window, cx);
    assert!(
        narrow_cap < 180.,
        "badge/font removal must lower the cap: {narrow_cap}"
    );
    assert!((live_width(&window, cx) - narrow_cap).abs() < 1.);
    cx.update(|cx| assert_eq!(prefs::get(cx, WIDTH_KEY).as_deref(), Some("180")));

    cx.update(|cx| {
        cx.global_mut::<MutableBadge>()
            .0
            .replace(Some("999999999999".into()));
        cx.set_global(Density::from_json(r#"{"--row-font":"16px"}"#));
        crate::theme::sync_font_sizes(cx);
    });
    draw(&window, cx);
    draw(&window, cx);
    assert!(cap(&window, cx) > 180.);
    assert!((live_width(&window, cx) - 180.).abs() < 1.);
    cx.update(|cx| assert_eq!(prefs::get(cx, WIDTH_KEY).as_deref(), Some("180")));
}

fn pointer_move(
    window: &mut Window,
    cx: &mut App,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    pressed_button: Option<MouseButton>,
) {
    dispatch_frame(
        window,
        cx,
        MouseMoveEvent {
            position,
            pressed_button,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
    );
}

fn collapse_pointer(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
) -> (
    gpui_kit::Point<gpui_kit::Pixels>,
    gpui_kit::Point<gpui_kit::Pixels>,
) {
    let bounds = cx
        .update_window((*window).into(), |_, window, _| {
            window.find(("vertical-dashboard-tab", 0usize)).bounds()
        })
        .unwrap();
    let start = point(bounds.right() + px(EDGE_INSET), bounds.center().y);
    let end = point(
        bounds.left() + px(collapsed_rail_width() - EDGE_INSET),
        bounds.center().y,
    );
    cx.update_window((*window).into(), |_, window, cx| {
        pointer_move(window, cx, start, None);
        pointer_down(window, cx, start);
        drag_moves(window, cx, start, end, 1..=MID_DRAG_END);
    })
    .unwrap();
    (start, end)
}

fn dispatch_frame(window: &mut Window, cx: &mut App, input: gpui_kit::PlatformInput) {
    window.dispatch_event(input, cx);
    window.render_frame(cx);
}

fn finish_collapse(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
    start: gpui_kit::Point<gpui_kit::Pixels>,
    end: gpui_kit::Point<gpui_kit::Pixels>,
) {
    cx.update_window((*window).into(), |_, window, cx| {
        drag_moves(window, cx, start, end, (MID_DRAG_END + 1)..=DRAG_STEPS)
    })
    .unwrap();
    window
        .update(cx, |app, _, cx| {
            assert!(
                !app.rail_shows_labels(cx),
                "labels must hide before mouse-up"
            );
            assert_eq!(prefs::get(cx, WIDTH_KEY).as_deref(), Some("180"));
        })
        .unwrap();
    release_pointer(window, cx, end);
    window
        .update(cx, |app, _, cx| {
            assert!(app.navigation_collapsed);
            assert_eq!(prefs::get(cx, COLLAPSED_KEY).as_deref(), Some("1"));
        })
        .unwrap();
}

fn outward_pointer(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
    cap: f32,
) -> gpui_kit::Point<gpui_kit::Pixels> {
    let bounds = cx
        .update_window((*window).into(), |_, window, _| {
            window.find(("vertical-dashboard-tab", 0usize)).bounds()
        })
        .unwrap();
    let start = point(bounds.right() + px(EDGE_INSET), bounds.center().y);
    let end = point(start.x + px(cap + OUTWARD_MARGIN), start.y);
    cx.update_window((*window).into(), |_, window, cx| {
        pointer_move(window, cx, start, None);
        pointer_down(window, cx, start);
        drag_moves(window, cx, start, end, 1..=DRAG_STEPS);
    })
    .unwrap();
    end
}

fn release_pointer(
    window: &WindowHandle<SubspaceApp>,
    cx: &mut TestAppContext,
    position: gpui_kit::Point<gpui_kit::Pixels>,
) {
    cx.update_window((*window).into(), |_, window, cx| {
        pointer_up(window, cx, position)
    })
    .unwrap();
}

fn pointer_down(window: &mut Window, cx: &mut App, position: gpui_kit::Point<gpui_kit::Pixels>) {
    dispatch_frame(
        window,
        cx,
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
    );
}

fn pointer_up(window: &mut Window, cx: &mut App, position: gpui_kit::Point<gpui_kit::Pixels>) {
    dispatch_frame(
        window,
        cx,
        MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }
        .to_platform_input(),
    );
}

fn drag_moves(
    window: &mut Window,
    cx: &mut App,
    start: gpui_kit::Point<gpui_kit::Pixels>,
    end: gpui_kit::Point<gpui_kit::Pixels>,
    steps: std::ops::RangeInclusive<u32>,
) {
    let total = DRAG_STEPS as f32;
    for step in steps {
        let fraction = step as f32 / total;
        pointer_move(
            window,
            cx,
            point(
                start.x + (end.x - start.x) * fraction,
                start.y + (end.y - start.y) * fraction,
            ),
            Some(MouseButton::Left),
        );
    }
}

fn draw(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext) {
    cx.update_window((*window).into(), |_, window, cx| window.render_frame(cx))
        .unwrap();
}

fn cap(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> f32 {
    window
        .update(cx, |app, window, cx| app.rail_content_max(window, cx))
        .unwrap()
}

fn live_width(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> f32 {
    window
        .update(cx, |app, _, cx| app.live_rail_width(cx).unwrap_or_default())
        .unwrap()
}
