use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gpui_kit::TestAppContext;
use serde_json::json;
use subspace_api_client::hub::IssueStatus;
use subspace_framework::dashboard::ListDashboard as _;

use super::offline::{Offline, board_with_hub, offline_with, row, settle_board};
use super::{has, press, render};
use crate::Issues;

const TRACKER: &str = "activity:trackers:1";
const RECENT: &str = "activity:recent:2";
const CONTINUED: &str = "activity:recent:3";
const UNKNOWN: &str = "activity:recent:4";
const CURSOR: &str = "recent-more";

#[gpui_kit::test]
fn activity_uses_the_same_issue_view_and_keeps_both_list_selections(cx: &mut TestAppContext) {
    let (fixture, _) = activity_fixture();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    press(cx, "j");
    press(cx, "j");
    press(cx, "5");
    settle_board(&board, cx, |board| {
        has_entry(board, TRACKER) && has_entry(board, RECENT)
    });
    render(cx);
    assert!(!has(cx, "issues-search"));
    cx.update(|_, cx| {
        let board = board.read(cx);
        let groups = board.list().groups();
        assert_eq!(groups[0].name.as_ref(), "Trackers in progress");
        assert_eq!(groups[0].count.as_deref(), Some("1"));
        assert!(groups[1].name.starts_with("Recently updated"));
        assert_eq!(groups[1].count, None);
        let recent = board.list().rows().find(|row| row.key == RECENT).unwrap();
        let accessible = board.row_accessible_label(recent, cx);
        assert!(accessible.contains("Comment added · Fixture worker (agent)"));
    });

    press(cx, "j");
    press(cx, "j");
    press(cx, "enter");
    settle_board(&board, cx, |board| {
        board.issue_view.snapshot.as_ref().is_some_and(|issue| {
            issue.identifier() == "SUB-1" && *issue.status() == IssueStatus::Loaded
        })
    });
    render(cx);
    assert!(has(cx, "issue-title"));
    press(cx, "escape");
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(board.activity.visible);
        assert_eq!(board.list().selected().map(AsRef::as_ref), Some(TRACKER));
    });
    press(cx, "2");
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(!board.activity.visible);
        assert_eq!(board.list().selected().map(AsRef::as_ref), Some("issue:1"));
    });
}

#[gpui_kit::test]
fn activity_paginates_and_retries_one_section(cx: &mut TestAppContext) {
    let (fixture, fail_older) = activity_fixture();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    press(cx, "5");
    settle_board(&board, cx, |board| {
        has_entry(board, TRACKER) && has_entry(board, RECENT)
    });
    fail_older.store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| {
        board.activate_activity_row("activity-notice:recent", cx)
    });
    settle_board(&board, cx, |board| {
        board.list().rows().any(|row| {
            row.key == "activity-notice:recent" && row.title.as_ref() == "Couldn't load more. Retry"
        })
    });
    cx.update(|_, cx| {
        assert!(has_entry(board.read(cx), TRACKER));
        assert!(has_entry(board.read(cx), RECENT));
    });
    fail_older.store(false, Ordering::Relaxed);
    board.update(cx, |board, cx| {
        board.activate_activity_row("activity-notice:recent", cx)
    });
    settle_board(&board, cx, |board| has_entry(board, CONTINUED));
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(has_entry(board, TRACKER));
        assert!(has_entry(board, RECENT));
        assert_eq!(board.list().groups()[1].count.as_deref(), Some("3"));
        let older = board
            .list()
            .rows()
            .find(|row| row.key == CONTINUED)
            .unwrap();
        assert!(
            board
                .row_accessible_label(older, cx)
                .contains("Review submitted")
        );
        let unknown = board.list().rows().find(|row| row.key == UNKNOWN).unwrap();
        let accessible = board.row_accessible_label(unknown, cx);
        assert!(accessible.contains("Updated"));
        assert!(!accessible.contains("new.kind"));
        assert!(
            !board
                .list()
                .rows()
                .any(|row| row.key == "activity-notice:recent")
        );
    });
}

fn has_entry(board: &Issues, key: &str) -> bool {
    board.activity.issue_identifier(key).is_some()
}

fn activity_fixture() -> (Offline, Arc<AtomicBool>) {
    let fail_older = Arc::new(AtomicBool::new(false));
    let failure = fail_older.clone();
    let offline = offline_with(move |request| {
        let path = &request.path;
        if !path.contains("order=activity") {
            return None;
        }
        let response = if path.contains("type=tracker") {
            json!({"items": [row("1", "Fixture tracker")], "nextCursor": null})
        } else if path.contains(CURSOR) {
            if failure.load(Ordering::Relaxed) {
                return Some((500, "{}".into()));
            }
            let older = with_activity(row("3", "Another fixture issue"), "review.submitted");
            let unknown = with_activity(row("4", "An unknown fixture event"), "new.kind");
            json!({"items": [older, unknown], "nextCursor": null})
        } else {
            let recent = with_activity(row("2", "Recently discussed fixture"), "comment.created");
            json!({"items": [recent], "nextCursor": CURSOR})
        };
        Some((200, response.to_string()))
    });
    (offline, fail_older)
}

fn with_activity(mut value: serde_json::Value, kind: &str) -> serde_json::Value {
    value["lastActivityAt"] = json!("2026-11-15T15:30:00Z");
    value["lastActivity"] = json!({
        "kind": kind, "occurredAt": "2026-11-15T15:30:00Z",
        "actor": {"type": "agent", "label": "Fixture worker"},
    });
    value
}
