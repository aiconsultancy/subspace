//! Search lists prefer identifiers and show matches in folded states.

mod fallback;

use gpui_kit::TestAppContext;
use subspace_framework::dashboard::ListDashboard as _;

use super::{design_groups, listed, requests};
use crate::{fixtures::issue, listing::More, requests::Request};

#[gpui_kit::test]
fn an_exact_match_in_done_replaces_other_results_once_its_page_loads(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    let mut groups = design_groups();
    requests(&board, cx);
    board.update(cx, |board, cx| {
        board.query = "sub-131".into();
        board.show_groups(groups.clone(), true, cx);
        assert!(!board.list().is_folded("group:state-done"));
    });
    assert!(requests(&board, cx).contains(&Request::Group {
        group: "state-done".into(),
    }));
    let done = &mut groups[2];
    done.rows.push(issue(
        "SUB-131",
        "Exact issue",
        ("Done", "completed"),
        serde_json::json!({}),
    ));
    done.more = More::Available;
    board.update(cx, |board, cx| {
        board.show_groups(groups, true, cx);
        assert_eq!(board.listing.issue_count(), Some(1));
        assert_eq!(board.list().groups().len(), 1);
        assert_eq!(
            board.list().groups()[0].rows[0].title.as_ref(),
            "Exact issue"
        );
        assert_eq!(board.list().groups()[0].rows.len(), 1);
        assert!(!board.list().is_folded("group:state-done"));
    });
}

#[gpui_kit::test]
fn searching_opens_populated_groups_without_changing_list_preferences(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    let mut groups = design_groups();
    groups[2].total = Some(0);
    groups[2].more = More::None;
    board.update(cx, |board, cx| {
        board.folds.record("state-started", groups[0].kind, true);
        board.query = "fixture words".into();
        board.show_groups(groups.clone(), true, cx);
        assert!(!board.list().is_folded("group:state-started"));
        assert!(board.list().is_folded("group:state-done"));
        assert_eq!(board.listing.issue_count(), Some(32));
        board.query.clear();
        board.show_groups(groups, true, cx);
        assert!(board.list().is_folded("group:state-started"));
    });
}

#[gpui_kit::test]
fn identifier_search_checks_later_pages_once_while_text_search_keeps_normal_paging(
    cx: &mut TestAppContext,
) {
    let (board, cx) = listed(cx);
    let mut groups = design_groups();
    groups[2].total = Some(0);
    groups[2].more = More::None;
    requests(&board, cx);
    board.update(cx, |board, cx| {
        board.query = "reference words".into();
        board.show_groups(groups.clone(), true, cx);
    });
    assert!(requests(&board, cx).is_empty());
    board.update(cx, |board, cx| {
        board.query = "sub-131".into();
        board.show_groups(groups.clone(), true, cx);
    });
    assert_eq!(
        requests(&board, cx),
        [Request::More {
            group: "state-todo".into()
        }]
    );
    board.update(cx, |board, cx| board.show_groups(groups, true, cx));
    assert!(
        requests(&board, cx).is_empty(),
        "the earlier page made no progress"
    );
}
