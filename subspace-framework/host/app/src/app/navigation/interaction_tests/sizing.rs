//! XL rail geometry and real pointer/keyboard collapse actions.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, Window, px};
use subspace_dashboard::icon_button::IconButtonSize;

use super::{draw, fixture::open_fixture};

#[gpui_kit::test]
fn extra_large_collapse_fits_the_rail_without_resizing_navigation_tabs(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let window = open_fixture(cx, root.path(), None, Some("180"));
    let app = window.update(cx, |_, _, cx| cx.entity()).unwrap();
    cx.update_window(*window, |_, window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(app.clone(), window, cx)
        });
    })
    .unwrap();
    cx.update(|cx| IconButtonSize::ExtraLarge.apply(cx));
    for _ in 0..4 {
        draw(&window, cx);
        cx.run_until_parked();
    }
    cx.update_window(*window, |_, window, cx| {
        app.update(cx, |app, cx| app.toggle_navigation_collapsed(window, cx));
    })
    .unwrap();
    for _ in 0..4 {
        draw(&window, cx);
        cx.run_until_parked();
    }
    cx.update_window(*window, |_, window, cx| {
        assert_collapse_geometry(window);
        window.click("toggle-board-navigation", cx);
    })
    .unwrap();
    assert!(!app.read_with(cx, |app, _| app.navigation_collapsed));
    cx.update_window(*window, |_, window, cx| {
        for _ in 0..64 {
            window.press("tab", cx);
            if window.find("toggle-board-navigation").focused() == Some(true) {
                break;
            }
        }
        assert_eq!(window.find("toggle-board-navigation").focused(), Some(true));
        window.press("space", cx);
    })
    .unwrap();
    assert!(app.read_with(cx, |app, _| app.navigation_collapsed));
}

fn assert_collapse_geometry(window: &Window) {
    let rail = window.find("board-navigation-rail").bounds();
    let footer = window.find("board-navigation-footer").bounds();
    let toggle = window.find("toggle-board-navigation").bounds();
    let tab = window.find(("vertical-dashboard-tab", 0usize));
    let target = IconButtonSize::ExtraLarge
        .metrics()
        .target()
        .to_pixels(window.rem_size());
    let device_pixel = px(1. / window.scale_factor());
    assert!((toggle.size.height - target).abs() <= device_pixel);
    assert!((toggle.size.width - target).abs() <= device_pixel);
    assert!(toggle.left() >= rail.left() && toggle.right() <= rail.right());
    assert!(toggle.top() >= footer.top() && toggle.bottom() <= footer.bottom());
    assert_eq!(tab.bounds().size.height, px(32.));
}
