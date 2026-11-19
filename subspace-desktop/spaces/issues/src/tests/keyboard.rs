//! The board's keys and focus (spec section 10): `j`/`k`, `1`–`4`,
//! `Shift+S`, `/` and `u`. Tests with a running hub cover issue opening.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_framework::dashboard::ListDashboard as _;

use super::{choice, listed, mount, press, render, requests, selected};
use crate::model::{SubIssues, Tab};
use crate::scope::Access;

#[gpui_kit::test]
fn digit_keys_choose_the_tabs_and_read_their_lists(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    for (key, tab) in [
        ("3", Tab::Backlog),
        ("1", Tab::MyIssues),
        ("4", Tab::All),
        ("2", Tab::Active),
    ] {
        press(cx, key);
        cx.update(|_, cx| assert_eq!(board.read(cx).tab, tab, "{key}"));
        assert_eq!(
            requests(&board, cx),
            [choice(tab, "", SubIssues::Hidden)],
            "{key}"
        );
    }
    cx.update(|_, cx| {
        assert_eq!(
            subspace_framework::dashboard::prefs::get(cx, "issues.tab").as_deref(),
            Some("active")
        );
    });
}

#[gpui_kit::test]
fn shift_s_shows_sub_issues_flat_and_latches_the_switch(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    press(cx, "shift-s");
    cx.update(|_, cx| assert_eq!(board.read(cx).sub_issues, SubIssues::Flat));
    assert_eq!(
        requests(&board, cx),
        [choice(Tab::Active, "", SubIssues::Flat)]
    );
    press(cx, "shift-s");
    cx.update(|_, cx| assert_eq!(board.read(cx).sub_issues, SubIssues::Hidden));
}

#[gpui_kit::test]
fn slash_focuses_search_and_enter_reads_and_returns_to_the_list(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    requests(&board, cx);
    press(cx, "/");
    cx.update(|window, _| {
        assert_eq!(window.find("issues-search").focused(), Some(true));
    });
    cx.update(|window, cx| window.input("fee amendments", cx));
    press(cx, "3");
    cx.update(|_, cx| {
        assert_eq!(board.read(cx).tab, Tab::Active, "digits type in the field");
    });
    press(cx, "enter");
    assert_eq!(
        requests(&board, cx),
        [choice(Tab::Active, "fee amendments3", SubIssues::Hidden)]
    );
    cx.update(|window, cx| {
        assert_ne!(window.find("issues-search").focused(), Some(true));
        assert!(board.read(cx).focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn escape_leaves_the_search_field(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    press(cx, "/");
    press(cx, "escape");
    cx.update(|window, cx| assert!(board.read(cx).focus.is_focused(window)));
}

#[gpui_kit::test]
fn j_and_k_move_through_groups_and_rows(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    press(cx, "j");
    assert_eq!(selected(&board, cx).as_deref(), Some("group:state-started"));
    press(cx, "j");
    assert_eq!(selected(&board, cx).as_deref(), Some("issue:id-SUB-955"));
    press(cx, "down");
    assert_eq!(selected(&board, cx).as_deref(), Some("issue:id-SUB-1540"));
    press(cx, "k");
    assert_eq!(selected(&board, cx).as_deref(), Some("issue:id-SUB-955"));
}

#[gpui_kit::test]
fn enter_without_a_hub_keeps_the_list_selection(cx: &mut TestAppContext) {
    let (board, cx) = listed(cx);
    press(cx, "j");
    press(cx, "j");
    requests(&board, cx);
    press(cx, "enter");
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert!(board.list().open_row().is_none());
        assert_eq!(
            board.list().selected().map(AsRef::as_ref),
            Some("issue:id-SUB-955")
        );
    });
    assert!(requests(&board, cx).is_empty());
}

#[gpui_kit::test]
fn list_keys_do_nothing_while_signed_out(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx);
    board.update(cx, |board, cx| {
        board.set_access(Access::Failed(crate::listing::Failure::Edge), cx)
    });
    render(cx);
    press(cx, "3");
    press(cx, "shift-s");
    cx.update(|_, cx| {
        let board = board.read(cx);
        assert_eq!(board.tab, Tab::Active);
        assert_eq!(board.sub_issues, SubIssues::Hidden);
    });
    assert!(requests(&board, cx).is_empty());
}
