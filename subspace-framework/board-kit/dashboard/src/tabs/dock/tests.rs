//! Real dock selection, visible indexing and native fallback.
mod drag;
mod fixture;
mod toolbar;

use fixture::{active, mount, settle};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

#[gpui_kit::test]
fn selecting_a_dock_tab_uses_the_native_group(cx: &mut TestAppContext) {
    let (area, _, cx) = mount(cx, false);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut bar = window.within("dock-resource-tabs");
        assert_eq!(bar.find(0usize).bounds().size.height, gpui_kit::px(32.));
        bar.click(1usize, cx);
        assert_eq!(bar.find(1usize).selected(), Some(true));
    });
    settle(cx);
    cx.read(|cx| assert_eq!(active(&area, cx), 1));
}

#[gpui_kit::test]
fn closing_an_inactive_dock_tab_preserves_selection_and_falls_back_to_native_title(
    cx: &mut TestAppContext,
) {
    let (area, _, cx) = mount(cx, false);
    for _ in 0..2 {
        cx.update(|window, cx| {
            let mut bar = window.within("dock-resource-tabs");
            bar.hover(1usize, cx);
            bar.within(1usize).click("close-tab", cx);
        });
        settle(cx);
        cx.read(|cx| assert_eq!(active(&area, cx), 0));
    }
    cx.update(|window, _| {
        assert!(window.try_find("dock-resource-tabs").is_none());
        assert!(window.within("tab-panel").find("menu").visible());
        assert!(window.within("tab").find("fixture-title").visible());
    });
}

#[gpui_kit::test]
fn visible_indices_skip_hidden_panels(cx: &mut TestAppContext) {
    let (area, _, cx) = mount(cx, true);
    cx.update(|window, cx| {
        window.render_frame(cx);
        let mut bar = window.within("dock-resource-tabs");
        assert_eq!(bar.find(0usize).label(), Some("Operations"));
        assert_eq!(bar.find(0usize).selected(), Some(true));
        bar.click(1usize, cx);
        assert_eq!(bar.find(1usize).label(), Some("Design"));
        assert_eq!(bar.find(1usize).selected(), Some(true));
    });
    settle(cx);
    cx.read(|cx| assert_eq!(active(&area, cx), 2));
}
