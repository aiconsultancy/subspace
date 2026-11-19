//! XL Contents disclosure retains pointer and keyboard focus behavior.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::{act, content, draw, long_guide, open};

#[gpui_kit::test]
fn contents_keyboard_toggle_and_back_restore_focus_at_extra_large(cx: &mut TestAppContext) {
    let (window, panel) = open(cx, content("keyboard", long_guide("Keyboard"), &[]));
    cx.update(|cx| subspace_dashboard::icon_button::IconButtonSize::ExtraLarge.apply(cx));
    draw(cx, window);
    act(cx, window, |window, cx| {
        window.click("help-contents-toggle", cx);
        window.render_frame(cx);
        assert_eq!(window.find("help-contents-toggle").focused(), Some(true));
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(false));
        assert!(!panel.read(cx).contents_open);
        window.press("space", cx);
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(true));
        assert!(panel.read(cx).contents_open);
        window.press("enter", cx);
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(false));
        window.press("enter", cx);
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(true));
        window.click("help-contents-keyboard-2", cx);
    });
    draw(cx, window);
    act(cx, window, |window, cx| {
        assert_eq!(
            window.find("help-dock-section-keyboard-2").focused(),
            Some(true)
        );
        window.click("help-dock-back-to-contents", cx);
        assert_eq!(window.find("help-contents-toggle").focused(), Some(true));
        assert_eq!(window.find("help-contents-toggle").expanded(), Some(true));
    });
}
