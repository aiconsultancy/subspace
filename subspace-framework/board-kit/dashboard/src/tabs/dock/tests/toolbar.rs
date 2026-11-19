use super::fixture::{mount, mount_with, settle};
use gpui_kit::TestAppContext;
use gpui_kit::component::dock::{DockPlacement, PanelControl};
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};

#[gpui_kit::test]
fn the_toolbar_keeps_panel_content_menu_zoom_and_close(cx: &mut TestAppContext) {
    let (area, panels, cx) = mount(cx, false);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("fixture-suffix").visible());
        window.click("fixture-action", cx);
        assert_eq!(panels[0].read(cx).invoked, 1);
        window.click("dock-zoom", cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(area.read(cx).is_zoomed());
        assert_eq!(window.find("dock-zoom").label(), Some("Zoom Out"));
        window.click("dock-menu", cx);
        assert!(window.find_by_label("Fixture action").visible());
        window.within("popup-menu").click(0usize, cx);
        assert_eq!(panels[0].read(cx).invoked, 2);
    });
    settle(cx);
    cx.update(|window, cx| {
        window.click("dock-menu", cx);
        window.within("popup-menu").click(2usize, cx);
    });
    settle(cx);
    cx.update(|window, cx| {
        assert!(!area.read(cx).is_zoomed());
        window.click("dock-menu", cx);
        assert!(window.find_by_label("Close").visible());
        window.within("popup-menu").click(4usize, cx);
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(
            window.within("dock-resource-tabs").find(0usize).label(),
            Some("Operations")
        );
    });
}

#[gpui_kit::test]
fn zoom_controls_respect_panel_policy_and_always_allow_zooming_out(cx: &mut TestAppContext) {
    let (area, panels, cx) = mount(cx, false);
    for (control, zoomable, toolbar, menu) in [
        (None, true, false, false),
        (Some(PanelControl::Menu), true, false, true),
        (Some(PanelControl::Toolbar), true, true, false),
        (Some(PanelControl::Both), false, false, false),
    ] {
        panels[0].update(cx, |panel, _| {
            panel.control = control;
            panel.zoomable = zoomable;
        });
        cx.update(|window, cx| {
            window.refresh();
            window.render_frame(cx);
        });
        cx.update(|window, cx| {
            assert_eq!(window.try_find("dock-zoom").is_some(), toolbar);
            window.click("dock-menu", cx);
            assert!(
                window
                    .within("popup-menu")
                    .find_by_label("Zoom In")
                    .visible()
            );
            window.within("popup-menu").click(2usize, cx);
            window.press("escape", cx);
        });
        settle(cx);
        assert_eq!(area.read_with(cx, |area, _| area.is_zoomed()), menu);
        if menu {
            cx.update(|window, cx| window.click("dock-zoom", cx));
            settle(cx);
        }
    }
    panels[0].update(cx, |panel, _| {
        panel.control = Some(PanelControl::Both);
        panel.zoomable = true;
    });
    cx.update(|window, cx| {
        window.refresh();
        window.render_frame(cx);
        window.click("dock-zoom", cx);
    });
    settle(cx);
    assert!(area.read_with(cx, |area, _| area.is_zoomed()));
    panels[0].update(cx, |panel, _| panel.control = None);
    cx.update(|window, cx| {
        window.refresh();
        window.render_frame(cx);
        assert_eq!(window.find("dock-zoom").label(), Some("Zoom Out"));
        window.click("dock-zoom", cx);
    });
    settle(cx);
    assert!(!area.read_with(cx, |area, _| area.is_zoomed()));
}

#[gpui_kit::test]
fn visible_dock_toggles_use_native_collapse_and_hide_while_zoomed(cx: &mut TestAppContext) {
    let (area, _, cx) = mount_with(cx, false, true);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("dock-toggle-Left").visible());
        assert!(window.find("dock-toggle-Right").visible());
        window.click("dock-toggle-Left", cx);
    });
    settle(cx);
    assert!(!area.read_with(cx, |area, _| area.is_dock_open(DockPlacement::Left)));
    cx.update(|window, cx| {
        assert_eq!(
            window.find("dock-toggle-Left").label(),
            Some("Expand Left dock")
        );
        window.click("dock-toggle-Left", cx);
        window.click("dock-zoom", cx);
    });
    settle(cx);
    assert!(area.read_with(cx, |area, _| area.is_dock_open(DockPlacement::Left)));
    cx.update(|window, _| {
        assert!(window.try_find("dock-toggle-Left").is_none());
        assert!(window.try_find("dock-toggle-Right").is_none());
    });
}
