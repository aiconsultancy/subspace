use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, px};
use subspace_dashboard::prefs;

use super::{HelpDockState, open_fixture, state};

#[gpui_kit::test]
fn the_native_resize_handle_changes_and_saves_the_preferred_width(cx: &mut TestAppContext) {
    let (window, fixture, _root) = open_fixture(None, cx);
    let dock = fixture.read_with(cx, |fixture, _| fixture.dock.clone());
    cx.update_window(window, |_, window, cx| {
        dock.update(cx, |host, cx| host.toggle(window, cx));
        window.render_frame(cx);
        let bounds = window.find("help-dock-scroll").bounds();
        let start = gpui_kit::point(bounds.left() + px(2.), bounds.center().y);
        window.drag(start, start - gpui_kit::point(px(60.), px(0.)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let saved = HelpDockState::decode(&prefs::get(cx, state::KEY).unwrap()).unwrap();
        assert!(
            (saved.width - 420.).abs() <= 2.,
            "native drag saved {}",
            saved.width
        );
        assert_eq!(dock.read(cx).state.width, saved.width);
    });
}
