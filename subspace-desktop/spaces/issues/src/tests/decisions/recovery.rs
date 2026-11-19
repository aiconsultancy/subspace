use super::super::press;
use super::{
    ALL, FIRST, NEW, NEXT, Offline, ROW, assert_compact_retry, choose, click, fixture::item, has,
    label, normal_fixture, offline_with, open, open_row, render, reveal_issue, settle_board,
    settle_detail, settle_page, switched,
};
use crate::{Issues, detail::timeline::TimelineMode};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU16, AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;
use subspace_api_client::hub::{DecisionReadStatus, IssueSnapshot, IssueStatus, ReadError};

#[gpui_kit::test]
fn first_visit_during_reload_waits_then_reads_the_first_page(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    fixture.switches.hold_issue.store(true, Ordering::Relaxed);
    reload(&board, cx);
    choose(&board, cx);
    render(cx);
    assert!(has(cx, "issue-decisions-loading"));
    assert!(super::super::labelled(cx, "Decisions, Loading"));
    assert!(!fixture.mock.requests().iter().any(|r| r.path == FIRST));
    fixture.switches.hold_issue.store(false, Ordering::Relaxed);
    settle_page(&board, cx);
    assert!(has(cx, ROW));
    assert_eq!(
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.path == FIRST)
            .count(),
        1
    );
}

#[gpui_kit::test]
fn each_opener_during_reload_resumes_when_the_issue_loads(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    for (mode, opener) in [(TimelineMode::Decisions, ROW), (TimelineMode::All, ALL)] {
        board.update(cx, |board, cx| board.show_timeline(mode, cx));
        if mode == TimelineMode::Decisions {
            settle_page(&board, cx);
        } else {
            settle_issue(&board, cx, |issue| issue.timeline().page().is_some());
        }
        fixture.switches.hold_issue.store(true, Ordering::Relaxed);
        reload(&board, cx);
        reveal_issue(&board, opener, cx);
        click(opener, cx);
        assert!(has(cx, "decision-panel-loading"));
        assert!(panel_read_pending(&board, cx, NEW));
        fixture.switches.hold_issue.store(false, Ordering::Relaxed);
        settle_detail(&board, cx, NEW);
        assert!(has(cx, "issue-decision:Made by"));
        press(cx, "escape");
    }
}

fn panel_read_pending(board: &Entity<Issues>, cx: &mut VisualTestContext, id: &str) -> bool {
    cx.update(|_, cx| {
        let issue = board.read(cx).issue_view.snapshot.as_ref().unwrap();
        *issue.status() == IssueStatus::Loading
            && matches!(issue.decision(id), Some(DecisionReadStatus::Loading))
    })
}

#[gpui_kit::test]
fn first_visit_offline_offers_retry_without_a_skeleton(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    set_offline(&fixture, true);
    fail_reload(&board, cx);
    choose(&board, cx);
    render(cx);
    assert!(has(cx, "issue-decisions-offline"));
    assert!(!has(cx, "issue-decisions-loading"));
    assert_compact_retry(
        cx,
        "issue-decisions-offline-retry",
        "issue-decisions-offline",
    );
    assert!(!fixture.mock.requests().iter().any(|r| r.path == FIRST));
    set_offline(&fixture, false);
    click_list(&board, "issue-decisions-offline-retry", cx);
    settle_page(&board, cx);
    assert!(has(cx, ROW));
    assert!(!has(cx, "issue-decisions-offline"));
}

#[gpui_kit::test]
fn cached_show_more_and_retry_recover_the_same_cursor_after_failed_issue_reloads(
    cx: &mut TestAppContext,
) {
    let next = Arc::new(AtomicU16::new(500));
    let fixture = switched(
        Arc::new(AtomicU16::new(200)),
        next.clone(),
        Arc::new(AtomicU16::new(200)),
    );
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    set_offline(&fixture, true);
    fail_reload(&board, cx);
    assert!(has(cx, ROW));
    click_list(&board, "issue-decisions-more", cx);
    settle_issue(&board, cx, is_offline);
    assert!(has(cx, ROW));
    assert!(has(cx, "issue-decisions-offline"));
    assert!(!fixture.mock.requests().iter().any(|r| r.path == NEXT));

    set_offline(&fixture, false);
    click_list(&board, "issue-decisions-more", cx);
    settle_issue(&board, cx, |issue| issue.decisions_failed().is_some());
    assert_eq!(
        label(cx, "issue-decisions-more-error").as_deref(),
        Some("Couldn’t load more decisions")
    );
    set_offline(&fixture, true);
    fail_reload(&board, cx);
    next.store(200, Ordering::Relaxed);
    set_offline(&fixture, false);
    click_list(&board, "issue-decisions-offline-retry", cx);
    settle_issue(&board, cx, |issue| {
        issue.decisions().is_some_and(|p| p.items.len() == 2)
    });
    assert!(has(cx, ROW));
    assert!(has(
        cx,
        "issue-decision:00000000-0000-0000-0000-000000000002"
    ));
    assert_eq!(
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.path == NEXT)
            .count(),
        2
    );
}

#[gpui_kit::test]
fn offline_show_more_is_disabled_while_retry_can_resume_a_refresh(cx: &mut TestAppContext) {
    let (fixture, started, release) = hold_refresh();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    reload(&board, cx);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    set_offline(&fixture, true);
    fail_reload(&board, cx);
    assert!(has(cx, ROW));
    assert!(has(cx, "issue-decisions-offline"));
    assert_more_disabled(cx);

    set_offline(&fixture, false);
    click_list(&board, "issue-decisions-more", cx);
    assert!(has(cx, "issue-decisions-offline"));
    assert_more_disabled(cx);
    click_list(&board, "issue-decisions-offline-retry", cx);
    settle_issue(&board, cx, |issue| {
        issue.decisions().unwrap().items[0].title == "Recovered"
    });
    release.send(()).unwrap();
    assert!(!has(cx, "issue-decisions-offline"));
    assert_eq!(
        label(cx, "issue-decisions-more").as_deref(),
        Some("Show more decisions")
    );
    assert_eq!(
        fixture
            .mock
            .requests()
            .iter()
            .filter(|r| r.path == FIRST)
            .count(),
        3
    );
    assert!(!fixture.mock.requests().iter().any(|r| r.path == NEXT));
}

fn assert_more_disabled(cx: &mut VisualTestContext) {
    cx.update(|window, _| {
        let button = window.find("issue-decisions-more");
        assert_eq!(button.label(), Some("Show more decisions"));
        assert_eq!(button.disabled(), Some(true));
    });
}

fn hold_refresh() -> (Offline, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (start, started) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let held = Mutex::new(held);
    let reads = AtomicUsize::new(0);
    let fixture = offline_with(move |r| {
        if r.path != FIRST {
            return None;
        }
        let title = match reads.fetch_add(1, Ordering::Relaxed) {
            0 => "Original",
            1 => {
                start.send(()).unwrap();
                held.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                "Obsolete"
            }
            _ => "Recovered",
        };
        let body = json!({"items":[item(NEW, title, "accepted")],"nextCursor":"next"});
        Some((200, body.to_string()))
    });
    (fixture, started, release)
}

#[gpui_kit::test]
fn a_panel_opened_offline_retries_without_leaving_the_panel(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    set_offline(&fixture, true);
    fail_reload(&board, cx);
    open_row(&board, cx);
    assert!(has(cx, "decision-panel-offline"));
    assert!(!has(cx, "decision-panel-loading"));
    set_offline(&fixture, false);
    click("decision-panel-retry", cx);
    settle_issue(&board, cx, |issue| {
        matches!(issue.decision(NEW), Some(DecisionReadStatus::Loaded(_)))
    });
    assert!(has(cx, "issue-decision:Made by"));
    assert!(!has(cx, "decision-panel-offline"));
}

fn click_list(board: &Entity<Issues>, id: &str, cx: &mut VisualTestContext) {
    reveal_issue(board, id, cx);
    click(id, cx);
}

fn set_offline(fixture: &Offline, offline: bool) {
    fixture
        .switches
        .offline_issue
        .store(offline, Ordering::Relaxed);
}

fn reload(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| board.reload_detail(cx));
    render(cx);
}

fn fail_reload(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    reload(board, cx);
    settle_issue(board, cx, is_offline);
}

fn is_offline(issue: &IssueSnapshot) -> bool {
    *issue.status() == IssueStatus::Error(ReadError::Network)
}

fn settle_issue(
    board: &Entity<Issues>,
    cx: &mut VisualTestContext,
    condition: impl Fn(&IssueSnapshot) -> bool,
) {
    settle_board(board, cx, |board| {
        board.issue_view.snapshot.as_ref().is_some_and(&condition)
    });
    render(cx);
}
