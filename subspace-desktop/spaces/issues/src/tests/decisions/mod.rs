mod fixture;
mod history;
mod keyboard;
mod panel;
mod recovery;

use super::offline::{Offline, board_with_hub, offline_with, settle_board};
use super::{has, label, render};
use crate::{Issues, detail::timeline::TimelineMode};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, ScrollHandle, TestAppContext, VisualTestContext, point, px};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};

pub(super) const NEW: &str = "00000000-0000-0000-0000-000000000001";
pub(super) use fixture::normal_fixture;
use fixture::{LONG_LINKS, fixture, long_fixture, switched};
const OLD: &str = "00000000-0000-0000-0000-000000000002";
const FIRST: &str = "/api/v1/decisions?issue=SUB-1&limit=50";
const NEXT: &str = "/api/v1/decisions?issue=SUB-1&limit=50&cursor=next";
const ROW: &str = "issue-decision:00000000-0000-0000-0000-000000000001";
const ALL: &str = "issue-activity-decision:event-decision";

fn open<'a>(
    fixture: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(fixture, cx);
    open_issue(&board, cx);
    (board, cx)
}

fn open_issue(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| board.open_detail("SUB-1".into(), cx));
    settle_board(board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| issue.timeline().page().is_some())
    });
    render(cx);
}

pub(super) fn choose(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        board.show_timeline(TimelineMode::Decisions, cx)
    });
}

pub(super) fn settle_page(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    settle_board(board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .is_some_and(|issue| issue.decisions().is_some())
    });
    render(cx);
}

fn click(id: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(id.to_owned(), cx));
    render(cx);
}

fn assert_compact_retry(cx: &mut VisualTestContext, retry: &'static str, notice: &'static str) {
    cx.update(|window, _| {
        let button = window.find(retry).bounds();
        let notice = window.find(notice).bounds();
        assert!(button.size.width < notice.size.width);
        assert_eq!(button.origin.x, notice.origin.x);
    });
}

fn reveal(id: &str, scroll: &ScrollHandle, cx: &mut VisualTestContext) {
    cx.update(|window, _| {
        let target = window.find(id.to_owned()).bounds().center().y;
        let viewport = scroll.bounds().center().y;
        let y = scroll.offset().y + viewport - target;
        scroll.set_offset(point(px(0.), y.clamp(-scroll.max_offset().y, px(0.))));
    });
    render(cx);
}

fn reveal_issue(board: &Entity<Issues>, id: &str, cx: &mut VisualTestContext) {
    let scroll = cx.update(|_, cx| board.read(cx).issue_view.scroll.clone());
    reveal(id, &scroll, cx);
}

pub(super) fn open_row(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    reveal_issue(board, ROW, cx);
    click(ROW, cx);
}

pub(super) fn settle_detail(board: &Entity<Issues>, cx: &mut VisualTestContext, id: &str) {
    use subspace_api_client::hub::DecisionReadStatus;
    settle_board(board, cx, |board| {
        board.issue_view.snapshot.as_ref().is_some_and(|issue| {
            issue
                .decision(id)
                .is_some_and(|status| !matches!(status, DecisionReadStatus::Loading))
        })
    });
    render(cx);
}

#[gpui_kit::test]
fn the_tab_is_lazy_keeps_decided_dates_and_hides_the_composer(cx: &mut TestAppContext) {
    let fixture = normal_fixture();
    let (board, cx) = open(&fixture, cx);
    assert!(has(cx, "issue-comment-composer"));
    assert!(!has(cx, ALL));
    assert!(
        !fixture
            .mock
            .requests()
            .iter()
            .any(|r| r.path.contains("/decisions"))
    );
    choose(&board, cx);
    settle_page(&board, cx);
    assert!(!has(cx, "issue-comment-composer"));
    assert_eq!(
        label(cx, ROW).as_deref(),
        Some("Open decision Read decisions here, Accepted, decided 12 November 2026, project SUB")
    );
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
        1
    );
}

#[gpui_kit::test]
fn a_continuation_error_preserves_rows_and_retry_reads_its_cursor(cx: &mut TestAppContext) {
    let code = Arc::new(AtomicU16::new(500));
    let fixture = switched(
        Arc::new(AtomicU16::new(200)),
        code.clone(),
        Arc::new(AtomicU16::new(200)),
    );
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_page(&board, cx);
    reveal_issue(&board, "issue-decisions-more", cx);
    click("issue-decisions-more", cx);
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .decisions_failed()
            .is_some()
    });
    render(cx);
    assert!(has(cx, ROW));
    assert!(has(cx, "issue-decisions-more-error"));
    assert_eq!(
        label(cx, "issue-decisions-retry").as_deref(),
        Some("Retry decisions")
    );
    code.store(200, Ordering::Relaxed);
    reveal_issue(&board, "issue-decisions-retry", cx);
    click("issue-decisions-retry", cx);
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .decisions()
            .is_some_and(|page| page.items.len() == 2)
    });
    render(cx);
    assert!(has(cx, ROW));
    assert!(!has(cx, "issue-decisions-more-error"));
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
fn a_first_page_error_can_retry_into_empty_without_paging(cx: &mut TestAppContext) {
    let code = Arc::new(AtomicU16::new(500));
    let fixture = switched(
        code.clone(),
        Arc::new(AtomicU16::new(200)),
        Arc::new(AtomicU16::new(200)),
    );
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .decisions_failed()
            .is_some()
    });
    render(cx);
    assert!(has(cx, "issue-decisions-error"));
    assert_compact_retry(cx, "issue-decisions-retry", "issue-decisions-error");
    assert_eq!(
        label(cx, "issue-decisions-error-hint").as_deref(),
        Some(crate::scope::read_error_message().as_str())
    );
    assert_eq!(
        label(cx, "issue-decisions-retry").as_deref(),
        Some("Retry decisions")
    );
    code.store(204, Ordering::Relaxed);
    reveal_issue(&board, "issue-decisions-retry", cx);
    click("issue-decisions-retry", cx);
    settle_page(&board, cx);
    assert!(has(cx, "issue-decisions-empty"));
    assert!(!has(cx, "issue-decisions-retry"));
}

#[gpui_kit::test]
fn denied_decisions_show_no_retry(cx: &mut TestAppContext) {
    let fixture = fixture(403, 200, 200);
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    settle_board(&board, cx, |board| {
        board
            .issue_view
            .snapshot
            .as_ref()
            .unwrap()
            .decisions_unavailable()
    });
    render(cx);
    assert!(has(cx, "issue-decisions-unavailable"));
    assert!(!has(cx, "issue-decisions-retry"));
}

#[gpui_kit::test]
fn the_tab_shows_loading_while_its_first_page_is_out(cx: &mut TestAppContext) {
    let (start, seen) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel();
    let held = std::sync::Mutex::new(held);
    let fixture = offline_with(move |r| {
        if r.path != FIRST {
            return None;
        }
        start.send(()).unwrap();
        held.lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        Some((200, json!({"items":[],"nextCursor":null}).to_string()))
    });
    let (board, cx) = open(&fixture, cx);
    choose(&board, cx);
    seen.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    render(cx);
    assert!(has(cx, "issue-decisions-loading"));
    assert!(super::labelled(cx, "Decisions, Loading"));
    release.send(()).unwrap();
    settle_page(&board, cx);
}
