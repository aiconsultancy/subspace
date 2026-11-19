//! Compact native Help title geometry across the global size choices.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, px, rems};
use subspace_dashboard::icon_button::IconButtonSize;

use super::{ToggleHelp, open};

#[gpui_kit::test]
fn compact_help_close_fits_the_native_header_at_every_global_size(cx: &mut TestAppContext) {
    for size in IconButtonSize::ALL {
        let (window, app, _root) = open(cx);
        cx.update(|cx| size.apply(cx));
        cx.update_window(window, |_, window, cx| {
            app.update(cx, |app, cx| app.toggle_help(&ToggleHelp, window, cx));
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let close_control = window.find("help-dock-close");
            assert_eq!(close_control.label(), Some("Close"));
            let close = close_control.bounds();
            let content = window.find("help-dock").bounds();
            let toolbar = window.find("app-toolbar").bounds();
            let target = rems(1.25).to_pixels(window.rem_size());
            // Root applies the theme's rem; layout rounds the compact relative target.
            let device_pixel = px(1. / window.scale_factor());
            assert!(
                (close.size.height - target).abs() <= device_pixel,
                "{size:?}"
            );
            assert!(close.size.width >= target - device_pixel, "{size:?}");
            assert!(close.top() >= toolbar.bottom(), "{size:?}: {close:?}");
            assert!(
                close.bottom() <= content.top(),
                "{size:?}: {close:?}, {content:?}"
            );
            window.click("help-dock-close", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(!app.read_with(cx, |app, _| app.help_open), "{size:?}");
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("help-alpha").focused(), Some(true), "{size:?}");
        })
        .unwrap();
    }
}
