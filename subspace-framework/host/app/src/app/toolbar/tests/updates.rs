//! About keeps the same command when its update indication changes.

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext, px, size};
use subspace_lucide::IconName;

use super::super::chrome::about_presentation;
use super::status::rooted_fixture;
use super::{ToolbarControl, popup_item};

#[test]
fn available_update_changes_about_presentation_without_becoming_an_install_command() {
    assert_eq!(about_presentation(false), (IconName::Info, "About"));
    assert_eq!(
        about_presentation(true),
        (IconName::Download, "About · Update available")
    );
}

#[gpui_kit::test]
fn shared_availability_refreshes_status_about_and_the_existing_label_menu(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = rooted_fixture(cx, root.path(), 2200., false);
    let original = cx
        .update_window(window, |_, window, _| {
            assert_eq!(window.find("about").label(), Some("About"));
            window.find("status-version").label().unwrap().to_owned()
        })
        .unwrap();
    set_available(cx, Some("1.2.0"));
    let icon_width = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let version = window.find("status-version");
            assert_eq!(
                version.label(),
                Some(format!("{original} · Update 1.2.0 available").as_str())
            );
            assert!(version.visible());
            assert_eq!(
                window.find("about").label(),
                Some("About · Update available")
            );
            let width = window.find("about").bounds().size.width;
            window.right_click("about", cx);
            width
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.within("popup-menu").find(0usize).label(),
            Some("Show label")
        );
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(ToolbarControl::About.shows_label(cx)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("about").bounds().size.width > icon_width);
    })
    .unwrap();
    set_available(cx, None);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("status-version").label(),
            Some(original.as_str())
        );
        assert_eq!(window.find("about").label(), Some("About"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn available_overflow_about_matches_inline_and_opens_about_without_installing(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = rooted_fixture(cx, root.path(), 2200., true);
    set_available(cx, Some("1.2.0"));
    let inline = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find("about").label().unwrap().to_owned()
        })
        .unwrap();
    resize(window, cx, 260.);
    cx.update_window(window, |_, window, cx| {
        window
            .within("global-toolbar")
            .click("overflow-toolbar-button", cx);
        let about = popup_item(window, "About · Update available");
        assert_eq!(window.find(about).label(), Some(inline.as_str()));
        window.click(about, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        assert_eq!(
            crate::app::available_update_version(cx).as_deref(),
            Some("1.2.0")
        );
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    set_available(cx, None);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("global-toolbar")
            .click("overflow-toolbar-button", cx);
        assert_eq!(
            window.find(popup_item(window, "About")).label(),
            Some("About")
        );
    })
    .unwrap();
}

fn set_available(cx: &mut TestAppContext, version: Option<&str>) {
    cx.update(|cx| crate::app::set_available_for_test(version, cx));
    cx.run_until_parked();
}

fn resize(window: AnyWindowHandle, cx: &mut TestAppContext, width: f32) {
    cx.simulate_window_resize(window, size(px(width), px(600.)));
    for _ in 0..6 {
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
    }
}
