use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use subspace_framework::client_test_support::Request;

use crate::{
    Issues,
    board::SearchFallback,
    listing::Status,
    model::{SubIssues, Tab},
    tests::offline::{Offline, board_with_hub, offline_with, row, settle_board, wait_for},
};

const BACKLOG_TITLE: &str = "Review the fixture";

fn scoped(path: &str) -> bool {
    path.contains("stateType=")
}

fn probe(path: &str) -> bool {
    path.contains("q=") && path.contains("limitPerGroup=25") && !scoped(path)
}

fn search_read(request: &Request) -> bool {
    request.path.contains("/issues/groups?") && request.path.contains("q=")
}

fn results(items: Vec<Value>, count: u32) -> (u16, String) {
    let groups = json!([{
        "stateId": "state-backlog",
        "stateType": "backlog",
        "count": count,
        "items": items,
        "nextCursor": null
    }]);
    (
        200,
        json!({"groupBy": "state", "groups": groups}).to_string(),
    )
}

fn backlog_row() -> Value {
    let mut issue = row("4", BACKLOG_TITLE);
    issue["state"] = json!({"id": "state-backlog", "name": "Backlog", "type": "backlog"});
    issue
}

fn search_reply(path: &str) -> (u16, String) {
    let count_only = path.contains("limitPerGroup=0");
    if path.contains("q=failed") && probe(path) {
        (500, "{}".to_owned())
    } else if path.contains("q=present") && scoped(path) && !count_only {
        results(vec![row("1", "Already in Active")], 1)
    } else if path.contains("q=empty") || scoped(path) {
        results(Vec::new(), 0)
    } else if count_only {
        results(Vec::new(), 1)
    } else {
        results(vec![backlog_row()], 1)
    }
}

fn fixture(held: Arc<AtomicBool>) -> Offline {
    offline_with(move |request| {
        if !search_read(request) {
            return None;
        }
        let path = &request.path;
        if probe(path) {
            wait_for(|| !held.load(Ordering::Relaxed));
        }
        Some(search_reply(path))
    })
}

fn open<'a>(
    offline: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    let (board, cx) = board_with_hub(offline, cx);
    settle_board(&board, cx, |board| {
        board.listing.status == Status::Idle && board.listing.content.is_some()
    });
    (board, cx)
}

fn search(board: &Entity<Issues>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        let input = board.read(cx).search.clone();
        input.update(cx, |input, cx| {
            input.replace_all(text.to_owned(), window, cx)
        });
    });
    board.update(cx, |board, cx| board.commit_search(cx));
}

fn wait_for_probe(board: &Entity<Issues>, cx: &mut VisualTestContext, offline: &Offline) {
    settle_board(board, cx, |board| {
        matches!(board.search_fallback, SearchFallback::Checking { .. })
    });
    cx.run_until_parked();
    wait_for(|| {
        offline
            .mock
            .requests()
            .iter()
            .any(|request| probe(&request.path))
    });
}

#[gpui_kit::test]
fn an_empty_active_search_offers_all_and_keeps_filters(cx: &mut TestAppContext) {
    let offline = fixture(Arc::default());
    let (board, cx) = open(&offline, cx);
    board.update(cx, |board, _| board.filters.priorities = vec![2]);
    search(&board, cx, "review");
    settle_board(&board, cx, |board| board.search_recovery().is_some());
    assert_eq!(board.read_with(cx, |board, _| board.tab), Tab::Active);
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.find("issues-search-reveal").visible());
        board.update(cx, |board, cx| board.reveal_search(cx));
    });
    settle_board(&board, cx, |board| {
        board.tab == Tab::All && board.listing.issue_count() == Some(1)
    });
    board.update(cx, |board, _| {
        assert_eq!(board.filters.priorities, [2]);
        assert_eq!(board.query, "review");
        assert_eq!(
            board.issue("issue:4").map(|row| row.fields.title.as_str()),
            Some(BACKLOG_TITLE)
        );
    });
    let probes: Vec<_> = offline
        .mock
        .requests()
        .into_iter()
        .filter(|request| probe(&request.path) && search_read(request))
        .collect();
    assert_eq!(probes.len(), 1);
    for pair in ["project=SUB", "priority=2", "parent=none", "q=review"] {
        assert!(probes[0].path.contains(pair), "probe keeps {pair}");
    }
}

#[gpui_kit::test]
fn search_can_reveal_a_sub_issue_in_the_nested_view(cx: &mut TestAppContext) {
    let offline = offline_with(|request| {
        if !search_read(request) {
            return None;
        }
        if request.path.contains("parent=none") {
            return Some(results(Vec::new(), 0));
        }
        let mut child = row("5", "A nested issue");
        child["parentId"] = json!("parent");
        Some(results(vec![child], 1))
    });
    let (board, cx) = open(&offline, cx);
    search(&board, cx, "nested");
    settle_board(&board, cx, |board| board.search_recovery().is_some());
    board.update(cx, |board, cx| {
        assert_eq!(board.search_recovery().unwrap().action(), "Show sub-issues");
        board.reveal_search(cx);
    });
    settle_board(&board, cx, |board| board.issue("issue:5").is_some());
    board.read_with(cx, |board, _| {
        assert_eq!(board.sub_issues, SubIssues::Nested);
        assert_eq!(board.tab, Tab::Active);
        assert_eq!(board.query, "nested");
    });
}

#[gpui_kit::test]
fn no_matches_or_a_failed_probe_keeps_the_search_in_active(cx: &mut TestAppContext) {
    let offline = fixture(Arc::default());
    let (board, cx) = open(&offline, cx);
    for query in ["empty", "failed"] {
        search(&board, cx, query);
        settle_board(&board, cx, |board| {
            board.listing.issue_count() == Some(0)
                && matches!(board.search_fallback, SearchFallback::Checked)
        });
        board.update(cx, |board, _| {
            assert_eq!(board.tab, Tab::Active);
            assert_eq!(board.listing.status, Status::Idle);
        });
    }
    let requests = offline.mock.requests();
    for query in ["q=empty", "q=failed"] {
        assert!(
            requests
                .iter()
                .any(|request| probe(&request.path) && request.path.contains(query))
        );
    }
}

#[gpui_kit::test]
fn a_match_in_active_does_not_check_all(cx: &mut TestAppContext) {
    let offline = fixture(Arc::default());
    let (board, cx) = open(&offline, cx);
    search(&board, cx, "present");
    settle_board(&board, cx, |board| board.listing.issue_count() == Some(1));
    board.update(cx, |board, _| {
        assert_eq!(board.tab, Tab::Active);
        assert!(matches!(board.search_fallback, SearchFallback::Idle));
    });
    assert!(
        !offline
            .mock
            .requests()
            .iter()
            .any(|request| probe(&request.path))
    );
}

#[gpui_kit::test]
fn a_manual_tab_choice_takes_precedence_over_a_pending_probe(cx: &mut TestAppContext) {
    let held = Arc::new(AtomicBool::new(true));
    let offline = fixture(held.clone());
    let (board, cx) = open(&offline, cx);
    search(&board, cx, "review");
    wait_for_probe(&board, cx, &offline);
    board.update(cx, |board, cx| board.set_tab(Tab::Backlog, cx));
    held.store(false, Ordering::Relaxed);
    settle_board(&board, cx, |board| board.search_recovery().is_some());
    board.read_with(cx, |board, _| {
        assert_eq!(board.tab, Tab::Backlog);
        assert_eq!(board.listing.issue_count(), Some(0));
    });
}

#[gpui_kit::test]
fn a_new_query_takes_precedence_over_a_pending_probe(cx: &mut TestAppContext) {
    let held = Arc::new(AtomicBool::new(true));
    let offline = fixture(held.clone());
    let (board, cx) = open(&offline, cx);
    search(&board, cx, "review");
    wait_for_probe(&board, cx, &offline);
    search(&board, cx, "present");
    held.store(false, Ordering::Relaxed);
    settle_board(&board, cx, |board| board.listing.issue_count() == Some(1));
    board.update(cx, |board, _| {
        assert_eq!(board.tab, Tab::Active);
        assert_eq!(board.query, "present");
        assert_eq!(
            board.issue("issue:1").map(|row| row.fields.title.as_str()),
            Some("Already in Active")
        );
    });
}
