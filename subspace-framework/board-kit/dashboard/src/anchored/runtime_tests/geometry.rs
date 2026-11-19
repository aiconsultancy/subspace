use super::{open, settle, setup};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, point, px};

#[gpui_kit::test]
fn anchored_fractional_bounds_keep_the_resolved_trigger_edge(cx: &mut TestAppContext) {
    for (host_offset, trigger_offset) in [(0.5, 40.), (0., 40.5)] {
        let (root, cx) = setup(cx);
        root.update(cx, |root, cx| {
            root.host_origin = point(px(host_offset), px(host_offset));
            root.position = point(px(trigger_offset), px(trigger_offset));
            cx.notify();
        });
        settle(cx);
        open(cx);
        cx.update(|window, _| {
            let trigger = window.find("anchored-trigger").bounds();
            let popup = window.find("anchored-body").bounds();
            assert_eq!(popup.top(), trigger.bottom());
            assert_eq!(popup.left(), trigger.left());
            assert!(!popup.intersects(&trigger));
        });
    }
}

#[gpui_kit::test]
fn anchored_popup_prefers_below_without_covering_trigger(cx: &mut TestAppContext) {
    let (_, cx) = setup(cx);
    open(cx);
    cx.update(|window, _| {
        let trigger = window.find("anchored-trigger").bounds();
        let popup = window.find("anchored-body").bounds();
        assert_eq!(popup.top(), trigger.bottom());
        assert_eq!(popup.left(), trigger.left());
        assert!(!popup.intersects(&trigger));
    });
}

#[gpui_kit::test]
fn anchored_bottom_right_popup_flips_above_and_clamps_horizontally(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    let viewport = cx.update(|window, _| window.viewport_size());
    root.update(cx, |root, cx| {
        root.position = point(viewport.width - px(110.), viewport.height - px(40.));
        cx.notify();
    });
    settle(cx);
    open(cx);
    cx.update(|window, _| {
        let trigger = window.find("anchored-trigger").bounds();
        let popup = window.find("anchored-body").bounds();
        assert_eq!(popup.bottom(), trigger.top(), "must flip, not corner-clamp");
        assert!(popup.right() <= viewport.width - px(8.));
        assert!(popup.left() >= px(8.));
        assert!(popup.left() < trigger.left(), "right edge must clamp");
        assert!(!popup.intersects(&trigger));
    });
}

#[gpui_kit::test]
fn anchored_open_popup_tracks_trigger_in_the_current_frame(cx: &mut TestAppContext) {
    let (root, cx) = setup(cx);
    open(cx);
    root.update(cx, |root, cx| {
        root.position = point(px(170.), px(200.));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| {
        let trigger = window.find("anchored-trigger").bounds();
        let popup = window.find("anchored-body").bounds();
        assert_eq!(popup.top(), trigger.bottom());
        assert_eq!(popup.left(), trigger.left());
    });
}
