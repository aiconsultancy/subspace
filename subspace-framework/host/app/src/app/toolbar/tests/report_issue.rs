//! Both real shell entry points must reach one retained report form.

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use subspace_dashboard::icon_button::IconButtonSize;

use super::ToolbarControl;
use super::status::rooted_fixture;

#[gpui_kit::test]
fn both_report_buttons_open_the_same_retained_form(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = rooted_fixture(cx, root.path(), 3200., false);
    cx.update(|cx| crate::test_support::install_offline_fixture_services(root.path(), cx));
    let mut retained = None;
    for id in ["report-issue", "status-report-issue"] {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find(id).label(), Some("Report issue"));
            window.click(id, cx);
        })
        .unwrap();
        cx.run_until_parked();
        let current = app.read_with(cx, |app, _| app.report_issue.as_ref().unwrap().entity_id());
        assert_eq!(*retained.get_or_insert(current), current);
        cx.update_window(window, |_, window, cx| {
            assert!(window.has_active_dialog(cx));
            window.render_frame(cx);
            assert!(window.try_find("report-review").is_some());
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    assert!(app.read_with(cx, |app, _| app.report_issue.is_some()));
}

/// The toolbar entry follows the global size; the status bar's is a compact
/// command like the version beside it.
#[gpui_kit::test]
fn report_entries_follow_their_bars(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = rooted_fixture(cx, root.path(), 5000., false);
    let mut toolbar_heights = Vec::new();
    for size in IconButtonSize::ALL {
        cx.update(|cx| size.apply(cx));
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            toolbar_heights.push(window.find("report-issue").bounds().size.height);
            let bottom = window.find("status-report-issue").bounds();
            let version = window.find("status-version").bounds();
            assert_eq!(bottom.size.height, version.size.height);
            assert_eq!(bottom.origin.y, version.origin.y);
            assert_eq!(window.find("report-issue").label(), Some("Report issue"));
            assert_eq!(
                window.find("status-report-issue").label(),
                Some("Report issue")
            );
        })
        .unwrap();
    }
    assert!(
        toolbar_heights.windows(2).all(|pair| pair[0] < pair[1]),
        "{toolbar_heights:?}"
    );
}

#[gpui_kit::test]
fn toolbar_report_label_follows_its_preference_while_the_status_bar_keeps_it(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let (window, _) = rooted_fixture(cx, root.path(), 3200., false);
    let (icon_width, status_width) = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let top = window.find("report-issue").bounds().size;
            let bottom = window.find("status-report-issue").bounds().size.width;
            assert!(top.width < bottom, "the toolbar control starts icon-only");
            window.right_click("report-issue", cx);
            (top.width, bottom)
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut menu = window.within("popup-menu");
        assert_eq!(menu.find(0usize).label(), Some("Show label"));
        menu.click(0usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(ToolbarControl::ReportIssue.shows_label(cx)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("report-issue").bounds().size.width > icon_width);
        assert_eq!(window.find("report-issue").label(), Some("Report issue"));
        let status = window.find("status-report-issue").bounds().size.width;
        assert_eq!(status, status_width);
    })
    .unwrap();
}

#[gpui_kit::test]
fn overflow_report_command_opens_the_report_form(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (window, app) = rooted_fixture(cx, root.path(), 600., true);
    cx.update(|cx| crate::test_support::install_offline_fixture_services(root.path(), cx));
    cx.update_window(window, |_, window, cx| {
        window
            .within("global-toolbar")
            .click("overflow-toolbar-button", cx);
        let item = (0usize..16)
            .find(|index| {
                window
                    .within("popup-menu")
                    .try_find(*index)
                    .is_some_and(|item| item.label() == Some("Report issue"))
            })
            .expect("Report issue must remain available in global overflow");
        window.within("popup-menu").click(item, cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.report_issue.is_some()));
    cx.update_window(window, |_, window, cx| {
        assert!(window.has_active_dialog(cx))
    })
    .unwrap();
}

/// With the feature but no repository, the app offers no Report issue: no
/// toolbar control, status bar command or working action.
#[gpui_kit::test]
fn without_a_repository_report_issue_is_hidden(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| crate::test_support::install_offline_fixture_services(root.path(), cx));
    let (window, app) = rooted_fixture(cx, root.path(), 3200., false);
    cx.update(crate::app::report_issue::remove_target);
    cx.update_window(window, |_, window, cx| {
        window.refresh();
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("report-issue").is_none());
        assert!(window.try_find("status-report-issue").is_none());
        assert!(window.try_find("about").is_some());
        window.dispatch_action(Box::new(crate::app::OpenReportIssue), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.report_issue.is_none()));
}
