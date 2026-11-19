use super::{has, label, listed, listed_at, mount, render, row_name, row_shown};
use crate::{listing::Failure, scope::Access};
use gpui_kit::TestAppContext;
use subspace_framework::dashboard::{Dashboard as _, ListDashboard as _};
const TRACKER: &str = "issue:id-SUB-955";
#[gpui_kit::test]
fn list_keeps_grouping_controls_and_accessible_rows_without_removed_actions(
    cx: &mut TestAppContext,
) {
    let (board, cx) = listed(cx);
    assert_eq!(
        label(cx, "issues-scope").as_deref(),
        Some("Subspace API · Signed in as Ada Fixture")
    );
    for id in [
        "issues-tabs",
        "issues-search",
        "issues-project-button",
        "issues-sub-issues",
    ] {
        assert!(has(cx, id), "{id}");
    }
    for id in ["issues-profile-button", "issue-new", "issues-sign-in"] {
        assert!(!has(cx, id), "{id}");
    }
    assert!(row_shown(cx, TRACKER));
    assert_eq!(
        row_name(&board, TRACKER, cx).as_deref(),
        Some("SUB-955, In Progress, No priority, Tracker: Feature module — Sample Tracking")
    );
    assert_eq!(label(cx, "issues-summary").as_deref(), Some("44 issues"));
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
    board.update(cx, |board, cx| {
        board.set_access(Access::Failed(Failure::Edge), cx)
    });
    render(cx);
    assert!(has(cx, "issues-retry"));
    assert!(!has(cx, "issues-sign-in"));
    assert!(!has(cx, "issues-tabs"));
}
#[gpui_kit::test]
fn offline_keeps_rows_and_narrow_windows_keep_issue_identity(cx: &mut TestAppContext) {
    let (board, cx) = listed_at(cx, 900.);
    board.update(cx, |board, cx| board.show_failure(Failure::Offline, cx));
    render(cx);
    assert!(row_shown(cx, TRACKER));
    assert_eq!(
        label(cx, "issues-scope").as_deref(),
        Some("Subspace API · Signed in as Ada Fixture")
    );
}
#[gpui_kit::test]
fn live_indicator_follows_the_updates_hub_once_signed_in(cx: &mut TestAppContext) {
    use subspace_api_client::hub::LiveStatus;
    let (board, cx) = listed(cx);
    for (live, words) in [
        (LiveStatus::Connecting, "Connecting…"),
        (LiveStatus::Live, "Live"),
        (LiveStatus::Reconnecting, "Reconnecting…"),
        (LiveStatus::Polling, "Polling every 30 s"),
    ] {
        board.update(cx, |board, cx| {
            board.live = live;
            cx.notify();
        });
        render(cx);
        assert_eq!(label(cx, "issues-live").as_deref(), Some(words));
    }
    board.update(cx, |board, cx| board.set_access(Access::Starting, cx));
    render(cx);
    assert!(!has(cx, "issues-live"));
}
