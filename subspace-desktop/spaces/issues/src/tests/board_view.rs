use super::{has, label, listed, listed_at, mount, render, row_name, row_shown};
use crate::{Issues, listing::Failure, scope::Access};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::{Dashboard as _, ListDashboard as _};
const TRACKER: &str = "issue:id-SUB-955";

/// The status bar's line for the board.
fn status(board: &Entity<Issues>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        board
            .read(cx)
            .status(cx)
            .map(|line| line.to_string())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn list_keeps_grouping_controls_and_accessible_rows_without_removed_actions(
    cx: &mut TestAppContext,
) {
    let (board, cx) = listed(cx);
    assert_eq!(
        status(&board, cx),
        "Subspace API · Signed in as Ada Fixture · Connecting… · 44 issues · updated just now",
        "the old heading's context is the status bar's"
    );
    for id in [
        "issues-workspace",
        "workspace-navigator",
        "issues-tabs",
        "issues-search",
        "issues-sub-issues",
        "issues-nav-view:active",
        "issues-nav-project:SUB",
        "issues-peek",
    ] {
        assert!(has(cx, id), "{id}");
    }
    for id in [
        "issues-profile-button",
        "issue-new",
        "issues-sign-in",
        "issues-heading",
        "issues-scope",
        "issues-live",
        "issues-project-button",
        "issues-view-button",
    ] {
        assert!(!has(cx, id), "{id}");
    }
    assert!(row_shown(cx, TRACKER));
    assert_eq!(
        row_name(&board, TRACKER, cx).as_deref(),
        Some("SUB-955, In Progress, No priority, Tracker: Feature module — Sample Tracking")
    );
    assert_eq!(
        label(cx, "issues-grouping").as_deref(),
        Some("Grouped by state"),
        "the count is the status bar's; the toolbar says how the list is grouped"
    );
    cx.update(|_, cx| {
        let current = board.read(cx);
        for group in current.list().groups() {
            assert!(crate::view::lanes::group_mark(current, group, cx).is_some());
        }
        assert_eq!(
            board.read(cx).window_title(cx).as_deref(),
            Some("Subspace API")
        )
    });
}
#[gpui_kit::test]
fn first_load_and_edge_failure_show_recovery_without_sign_in_controls(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    render(cx);
    assert!(has(cx, "issues-loading"));
    assert!(!has(cx, "issues-tabs"));
    assert!(!has(cx, "issues-peek"), "nothing to peek at yet");
    board.update(cx, |board, cx| {
        board.set_access(Access::Failed(Failure::Edge), cx)
    });
    render(cx);
    assert!(has(cx, "issues-retry"));
    assert!(!has(cx, "issues-sign-in"));
    assert!(!has(cx, "issues-tabs"));
    assert_eq!(status(&board, cx), "Subspace API · Sign-in required");
}
#[gpui_kit::test]
fn offline_keeps_rows_and_narrow_windows_keep_issue_identity(cx: &mut TestAppContext) {
    let (board, cx) = listed_at(cx, 900.);
    board.update(cx, |board, cx| board.show_failure(Failure::Offline, cx));
    render(cx);
    assert!(row_shown(cx, TRACKER));
    assert_eq!(
        status(&board, cx),
        "Subspace API · Signed in as Ada Fixture · Offline"
    );
}
#[gpui_kit::test]
fn the_status_bar_follows_the_updates_hub_once_signed_in(cx: &mut TestAppContext) {
    use subspace_api_client::hub::LiveStatus;
    let (board, cx) = listed(cx);
    for (live, words) in [
        (LiveStatus::Connecting, "Connecting…"),
        (LiveStatus::Live, "Live"),
        (LiveStatus::Reconnecting, "Reconnecting…"),
        (LiveStatus::Polling, "Polling every 30 s"),
        (LiveStatus::Off, "Live updates off: reload to refresh"),
    ] {
        board.update(cx, |board, cx| {
            board.live = live;
            cx.notify();
        });
        render(cx);
        assert!(
            status(&board, cx).starts_with(&format!(
                "Subspace API · Signed in as Ada Fixture · {words} · "
            )),
            "{words}"
        );
        assert!(!has(cx, "issues-live"), "no indicator of its own");
    }
    board.update(cx, |board, cx| board.set_access(Access::Starting, cx));
    render(cx);
    assert!(!status(&board, cx).contains("Live"));
}
