use crate::tests::offline::{board_with_hub, offline_with, settle_board};
use crate::tests::{has, label, render};
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use serde_json::json;

#[gpui_kit::test]
fn tracker_header_progress_exposes_the_fixture_counts_and_visible_text(cx: &mut TestAppContext) {
    let fixture = offline_with(|request| {
        if !request.path.ends_with("/issues/SUB-1612") {
            return None;
        }
        let mut detail = crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
        detail["progress"] = json!({"done": 9, "total": 25, "canceled": 2,
            "duplicate": 1, "sample": [], "hasMore": true});
        Some((200, detail.to_string()))
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    board.update(cx, |board, cx| board.open_detail("SUB-1612".into(), cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|s| s.detail().is_some())
    });
    render(cx);
    assert_eq!(
        label(cx, "issue-header-progress").as_deref(),
        Some("9 of 25 done. Not counted: 2 cancelled · 1 duplicate")
    );
    assert_eq!(
        label(cx, "issue-header-progress-text").as_deref(),
        Some("9 of 25 done")
    );
    assert_eq!(
        label(cx, "issue-header-progress-excluded").as_deref(),
        Some("Not counted: 2 cancelled · 1 duplicate")
    );
    cx.update(|window, _| {
        assert!(
            window
                .find("issue-header-progress-text")
                .bounds()
                .size
                .width
                > gpui_kit::px(0.)
        );
        assert!(
            window
                .find("issue-header-progress-excluded")
                .bounds()
                .size
                .height
                > gpui_kit::px(0.)
        );
    });
}

#[gpui_kit::test]
fn header_progress_switches_figures_and_hides_empty_exclusions(cx: &mut TestAppContext) {
    let fixture = offline_with(|request| {
        let total = match request.path.rsplit('/').next()? {
            "SUB-16" => 16,
            "SUB-17" => 17,
            _ => return None,
        };
        let mut detail = crate::tests::offline::api("/api/v1/issues/SUB-1", &Default::default());
        detail["identifier"] = json!(format!("SUB-{total}"));
        detail["progress"] = json!({"done": 9, "total": total, "canceled": 0,
            "duplicate": 0, "sample": [], "hasMore": false});
        Some((200, detail.to_string()))
    });
    let (board, cx) = board_with_hub(&fixture, cx);
    for (identifier, marks) in [("SUB-16", true), ("SUB-17", false)] {
        board.update(cx, |board, cx| board.open_detail(identifier.into(), cx));
        settle_board(&board, cx, |board| {
            board
                .issue_view
                .snapshot
                .as_ref()
                .and_then(|s| s.detail())
                .is_some_and(|d| d.fields.identifier == identifier)
        });
        render(cx);
        assert_eq!(has(cx, "issue-header-progress-marks"), marks);
        assert_eq!(has(cx, "issue-header-progress-bar"), !marks);
        assert!(!has(cx, "issue-header-progress-excluded"));
    }
}
