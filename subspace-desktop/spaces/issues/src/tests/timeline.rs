use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

use super::offline::{Offline, board_with_hub, offline_with, settle_board};
use super::{has, press, render, settle_frames};
use crate::{Issues, detail::timeline::TimelineMode};

const PAGES: &str = include_str!("../../fixtures/issue-timeline.json");
mod decision_text;
use decision_text::{assert_decision_text_has_no_uuid, assert_resolved_decision};

#[gpui_kit::test]
fn revealed_comments_can_load_earlier_pages_with_a_bounded_goal(cx: &mut TestAppContext) {
    let (fixture, _) = fixture();
    let (board, cx) = open_timeline(&fixture, cx);
    board.update(cx, |board, cx| {
        let items = &board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .timeline()
            .page()
            .unwrap()
            .items;
        board.issue_view.timeline.show_all_loaded(items);
        board.more_timeline(cx);
    });
    settle_board(&board, cx, |board| rows(board) == 9);
    cx.update(|_, cx| {
        let goal = board.read(cx).issue_view.timeline.read_goal();
        assert!(goal <= rows(board.read(cx)) + 6, "unbounded goal: {goal}");
    });
}

#[gpui_kit::test]
fn comments_are_default_and_all_includes_activity_until_opening_another_issue(
    cx: &mut TestAppContext,
) {
    let (fixture, _) = fixture();
    let (board, cx) = open_timeline(&fixture, cx);
    render(cx);
    assert!(has(cx, "timeline-comment:comment-event-1"));
    assert!(has(cx, "timeline-comment:comment-event-2"));
    assert!(!has(cx, "timeline-entry:comment-event-2"));
    cx.update(|window, _| {
        let parent = window.find("timeline-entry:comment-event-1").bounds();
        let reply = window.find("timeline-comment:comment-event-2").bounds();
        let next = window.find("timeline-entry:comment-event-3").bounds();
        assert!(parent.left() < reply.left());
        assert!(reply.bottom() <= parent.bottom());
        assert!(parent.bottom() <= next.top());
    });
    assert!(!has(cx, "timeline-event:event-2"));
    assert_eq!(mode(&board, cx), TimelineMode::Comments);
    press(cx, "m");
    settle_frames(cx);
    cx.update(|window, cx| {
        let tabs = &board
            .read(cx)
            .issue_view
            .timeline_focus
            .as_ref()
            .unwrap()
            .tabs;
        assert!(
            tabs.contains_focused(window, cx),
            "an Activity tab has focus"
        );
        assert!(!tabs.is_focused(window), "the selected tab, not the list");
    });

    board.update(cx, |board, cx| board.show_timeline(TimelineMode::All, cx));
    render(cx);
    assert!(has(cx, "timeline-comment:comment-event-1"));
    assert!(has(cx, "timeline-event:event-2"));
    assert!(has(cx, "timeline-entry:comment-event-2"));
    board.update(cx, |board, cx| board.open_detail("SUB-2".into(), cx));
    settle_board(&board, cx, |board| rows(board) == 8);
    assert_eq!(mode(&board, cx), TimelineMode::Comments);
}

#[gpui_kit::test]
fn timeline_retry_preserves_issue_and_comments_then_loads_the_tombstone(cx: &mut TestAppContext) {
    let (fixture, fail_older) = fixture();
    let (board, cx) = open_timeline(&fixture, cx);
    fail_older.store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| board.more_timeline(cx));
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .timeline()
            .error()
            .is_some()
    });
    render(cx);
    assert!(has(cx, "issue-title"));
    assert!(has(cx, "timeline-comment:comment-event-1"));
    assert!(has(cx, "issue-timeline-retry"));
    fail_older.store(false, Ordering::Relaxed);
    board.update(cx, |board, cx| board.retry_timeline(cx));
    settle_board(&board, cx, |board| rows(board) == 9);
    render(cx);
    assert!(has(cx, "timeline-tombstone:comment-removed"));
    assert!(!has(cx, "issue-timeline-retry"));
}

#[gpui_kit::test]
fn sparse_comments_stop_automatic_paging_and_can_continue(cx: &mut TestAppContext) {
    let pages: Value = serde_json::from_str(PAGES).unwrap();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let fixture = offline_with(move |request| {
        if !request.path.contains("/activity?limit=50") {
            return None;
        }
        let number = count.fetch_add(1, Ordering::Relaxed) + 1;
        let page = if number == 5 {
            pages["earlier"].clone()
        } else {
            let mut event = pages["head"]["items"][2].clone();
            event["id"] = json!(format!("sparse-event-{number}"));
            event["kind"] = json!("issue.changed");
            event["comment"] = Value::Null;
            json!({"items": [event], "nextCursor": format!("sparse-{number}")})
        };
        Some((200, page.to_string()))
    });
    let (board, cx) = open_issue(&fixture, cx);
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| rows(board) == 4 && !issue.timeline().loading())
    });
    assert_eq!(reads.load(Ordering::Relaxed), 4);
    board.update(cx, |board, cx| board.more_timeline(cx));
    settle_board(&board, cx, |board| rows(board) == 5);
    render(cx);
    assert!(has(cx, "timeline-tombstone:comment-removed"));
}

#[gpui_kit::test]
fn decisions_are_only_in_all_and_missing_references_have_no_open_action(cx: &mut TestAppContext) {
    let fixture = decision_fixture();
    let (board, cx) = open_issue(&fixture, cx);
    settle_board(&board, cx, |board| rows(board) == 11);
    render(cx);
    assert!(has(cx, "timeline-comment:comment-event-1"));
    assert!(!has(cx, "timeline-event:decision-event"));
    assert!(!has(cx, "issue-activity-decision:decision-event"));
    board.update(cx, |board, cx| board.show_timeline(TimelineMode::All, cx));
    render(cx);
    cx.update(|window, _| assert_resolved_decision(window, "Use page offsets", "Proposed"));
    assert!(has(cx, "timeline-event:missing-decision-event"));
    assert!(!has(cx, "issue-activity-decision:missing-decision-event"));

    board.update(cx, |board, cx| board.more_timeline(cx));
    settle_board(&board, cx, |board| rows(board) == 13);
    cx.update(|_, cx| board.read(cx).issue_view.scroll.scroll_to_bottom());
    render(cx);
    assert!(has(cx, "timeline-event:earlier-decision-event"));
    assert_decision_text_has_no_uuid(cx);
    board.update(cx, |board, cx| {
        board.show_timeline(TimelineMode::Comments, cx)
    });
    render(cx);
    assert!(has(cx, "timeline-comment:comment-event-1"));
    assert!(!has(cx, "timeline-event:decision-event"));
}

fn decision_fixture() -> Offline {
    let mut pages: Value = serde_json::from_str(PAGES).unwrap();
    let id = "00000000-0000-0000-0000-000000000001";
    let mut event = pages["head"]["items"][2].clone();
    event["id"] = json!("decision-event");
    event["kind"] = json!("decision.logged");
    event["comment"] = Value::Null;
    event["data"] = json!({"decisionId":id});
    pages["head"]["items"]
        .as_array_mut()
        .unwrap()
        .push(event.clone());
    let mut earlier = event.clone();
    earlier["id"] = json!("earlier-decision-event");
    earlier["occurredAt"] = json!("2026-11-15T05:00:00Z");
    earlier["displayAt"] = earlier["occurredAt"].clone();
    pages["earlier"]["items"]
        .as_array_mut()
        .unwrap()
        .push(earlier);
    pages["earlier"]["references"] =
        json!({"decisions":[{"id":id,"title":"Use cursor paging","status":"accepted"}]});
    event["id"] = json!("missing-decision-event");
    event["data"] = json!({"decisionId":"00000000-0000-0000-0000-000000000099"});
    pages["head"]["items"]
        .as_array_mut()
        .unwrap()
        .push(event.clone());
    event["id"] = json!("unknown-decision-event");
    event["data"] = json!({"decisionId":"ed2bbf41-7b38-47e2-8dd2-c30ed3d72c47"});
    pages["head"]["items"].as_array_mut().unwrap().push(event);
    pages["head"]["references"] = json!({"decisions":[
        {"id":id,"title":"Use page offsets","status":"proposed"},
        {"id":"ed2bbf41-7b38-47e2-8dd2-c30ed3d72c47","title":"Future decision","status":"future"}
    ]});
    offline_with(move |request| {
        let page = match request.path.as_str() {
            "/api/v1/issues/SUB-1/activity?limit=50" => "head",
            "/api/v1/issues/SUB-1/activity?limit=50&cursor=fixture-timeline-older" => "earlier",
            _ => return None,
        };
        Some((200, pages[page].to_string()))
    })
}

fn open_timeline<'a>(
    fixture: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    let (board, cx) = open_issue(fixture, cx);
    settle_board(&board, cx, |board| rows(board) == 8);
    (board, cx)
}

fn open_issue<'a>(
    fixture: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(fixture, cx);
    settle_board(&board, cx, |board| board.issue("issue:1").is_some());
    board.update(cx, |board, cx| board.open_row("issue:1", cx));
    (board, cx)
}

fn rows(board: &Issues) -> usize {
    board
        .issue_view
        .snapshot
        .as_ref()
        .and_then(|issue| issue.timeline().page())
        .map_or(0, |page| page.items.len())
}

fn mode(board: &gpui_kit::Entity<Issues>, cx: &mut gpui_kit::VisualTestContext) -> TimelineMode {
    cx.update(|_, cx| board.read(cx).issue_view.timeline.mode)
}

fn fixture() -> (Offline, Arc<AtomicBool>) {
    let pages: Value = serde_json::from_str(PAGES).unwrap();
    let fail_older = Arc::new(AtomicBool::new(false));
    let failure = fail_older.clone();
    let offline = offline_with(move |request| {
        if !request.path.contains("/activity?limit=50") {
            return None;
        }
        if request.path.contains("cursor=fixture-timeline-older") {
            if failure.load(Ordering::Relaxed) {
                return Some((500, "{}".into()));
            }
            return Some((200, pages["earlier"].to_string()));
        }
        Some((200, pages["head"].to_string()))
    });
    (offline, fail_older)
}

#[gpui_kit::test]
fn a_palette_command_selects_and_focuses_its_activity_tab(cx: &mut TestAppContext) {
    use subspace_framework::dashboard::CommandRun;
    let (fixture, _) = fixture();
    let (board, cx) = open_timeline(&fixture, cx);
    render(cx);
    for (label, expected, tab) in [
        ("Show issue decisions", TimelineMode::Decisions, 2usize),
        ("Show all issue activity", TimelineMode::All, 1usize),
    ] {
        cx.update(|window, cx| {
            let commands = board.read(cx).palette_commands();
            let command = commands
                .iter()
                .find(|command| command.label == label)
                .unwrap();
            let CommandRun::Callback(run) = &command.run else {
                panic!("expected a callback");
            };
            run(window, cx);
        });
        settle_frames(cx);
        assert_eq!(mode(&board, cx), expected);
        cx.update(|window, _| {
            let selected = window.within("issue-timeline-tabs").find(tab);
            assert_eq!(selected.focused(), Some(true), "{label} focuses its tab");
        });
    }
}
