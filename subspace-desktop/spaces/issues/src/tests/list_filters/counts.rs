//! The counts beside the list: My issues on its navigator row, and hidden
//! sub-issues on their switch and in the status bar.

use std::sync::atomic::Ordering;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{click, parameters};
use crate::Issues;
use crate::tests::offline::{board_with_hub, offline, settle_board};
use crate::tests::{has, label, labelled, listed, render};

/// The status bar's line for the board.
fn status(board: &Entity<Issues>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| board.read(cx).status_line(std::time::Instant::now()))
}

/// The count the navigator's My issues row shows.
fn my_issues_count(board: &Entity<Issues>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| {
        let board = board.read(cx);
        let current = crate::navigator::View::current(board.tab, board.activity.visible);
        crate::navigator::views(current, board.listing.my_issues_count)
            .into_iter()
            .next()
            .and_then(|entry| entry.count)
    })
}

#[gpui_kit::test]
fn real_hub_counts_show_in_the_navigator_the_switch_and_the_status_bar(cx: &mut TestAppContext) {
    let fixture = offline();
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| {
        board.listing.hidden_subissues_count.is_some() && board.listing.my_issues_count.is_some()
    });
    render(cx);
    assert_eq!(
        label(cx, "issues-sub-issues").as_deref(),
        Some("Sub-issues, 12 hidden"),
        "the switch says how many sub-issues are hidden"
    );
    assert!(status(&board, cx).ends_with(" · 12 sub-issues hidden"));
    assert!(has(cx, "issues-nav-view:my-issues"));
    assert_eq!(my_issues_count(&board, cx).as_deref(), Some("3"));
    click(cx, "issues-sub-issues");
    assert_eq!(
        cx.update(|_, cx| board.read(cx).sub_issues),
        crate::model::SubIssues::Flat
    );
    assert_eq!(
        label(cx, "issues-sub-issues").as_deref(),
        Some("Sub-issues")
    );
    assert!(!status(&board, cx).contains("sub-issues hidden"));
}

#[gpui_kit::test]
fn failed_counts_are_omitted_without_hiding_the_list(cx: &mut TestAppContext) {
    let fixture = offline();
    fixture.switches.fail_counts.store(true, Ordering::Relaxed);
    let (board, cx) = board_with_hub(&fixture, cx);
    settle_board(&board, cx, |board| {
        board.listing.content.is_some()
            && fixture
                .mock
                .requests()
                .iter()
                .filter(|read| read.path.contains("limitPerGroup=0"))
                .count()
                >= 2
    });
    render(cx);
    assert!(status(&board, cx).contains(" · 3 issues · "));
    assert!(labelled(cx, "My issues"));
    assert_eq!(my_issues_count(&board, cx), None, "no count to show");
    assert_eq!(
        label(cx, "issues-sub-issues").as_deref(),
        Some("Sub-issues")
    );
    assert!(!status(&board, cx).contains("sub-issues hidden"));
}

#[gpui_kit::test]
fn changing_the_list_clears_old_counts_and_status_filters(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    board.update(cx, |board, cx| {
        board.listing.my_issues_count = Some(8);
        board.listing.hidden_subissues_count = Some(12);
        board.filters.states = vec!["state-started".into()];
        board.set_tab(crate::model::Tab::Backlog, cx);
    });
    render(cx);
    assert!(labelled(cx, "My issues"));
    assert_eq!(
        label(cx, "issues-sub-issues").as_deref(),
        Some("Sub-issues")
    );
    assert!(!parameters(&board, cx).iter().any(|(key, _)| key == "state"));
}

#[gpui_kit::test]
fn my_issues_count_shows_on_its_navigator_row(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    for count in [0, 1, 2] {
        board.update(cx, |board, cx| {
            board.listing.my_issues_count = Some(count);
            cx.notify();
        });
        render(cx);
        assert!(has(cx, "issues-nav-view:my-issues"));
        assert_eq!(
            my_issues_count(&board, cx),
            Some(count.to_string()),
            "{count}"
        );
    }
}
