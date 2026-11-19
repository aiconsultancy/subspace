use gpui_kit::TestAppContext;
use subspace_framework::dashboard::Dashboard as _;

use super::{KEY, URL, mount, render, seed};

#[gpui_kit::test]
fn closing_an_out_of_list_pr_removes_it_before_section_changes(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    for key in ["escape", "left"] {
        board.update(cx, |board, cx| {
            assert!(board.open_link(URL, cx));
            assert_eq!(board.list.rows().count(), 1);
        });
        render(cx);
        board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
        cx.simulate_keystrokes(key);
        board.update(cx, |board, cx| {
            assert!(board.list.open_row().is_none());
            assert!(board.navigation.is_none());
            assert_eq!(board.list.rows().count(), 0);
            board.section_action(1, cx);
            board.rebuild(cx);
            assert!(!board.list.rows().any(|row| row.key == KEY));
        });
    }
}

#[gpui_kit::test]
fn changing_section_discards_an_open_out_of_list_pr(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    board.update(cx, |board, cx| {
        assert!(board.open_link(URL, cx));
        board.section_action(1, cx);
        assert!(board.navigation.is_none());
        assert!(board.list.open_row().is_none());
        assert_eq!(board.list.rows().count(), 0);
    });
}

#[gpui_kit::test]
fn selecting_a_different_row_discards_the_transient_row(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    board.update(cx, seed);
    board.update(cx, |board, cx| {
        assert!(board.open_link("https://github.com/octo-org/api/pull/999", cx));
        assert_eq!(board.list.rows().count(), 2);
    });
    render(cx);
    board.update_in(cx, |board, window, cx| board.focus.focus(window, cx));
    cx.simulate_keystrokes("k");
    board.read_with(cx, |board, _| {
        assert!(board.navigation.is_none());
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert_eq!(board.list.rows().count(), 1);
    });
}

#[gpui_kit::test]
fn search_selection_discards_the_transient_row(cx: &mut TestAppContext) {
    let (board, cx) = mount(cx, None);
    board.update(cx, seed);
    board.update_in(cx, |board, window, cx| {
        assert!(board.open_link("https://github.com/octo-org/api/pull/999", cx));
        board.open_search_result(KEY, window, cx);
        assert!(board.navigation.is_none());
        assert_eq!(board.list.open_row().unwrap().key.as_ref(), KEY);
        assert_eq!(board.list.rows().count(), 1);
    });
}
