use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use subspace_framework::client_github::model::PrState;

use super::{settle, setup};

#[gpui_kit::test]
fn panel_and_palette_offer_close_only_while_open(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx);
    for state in [
        PrState::Open,
        PrState::Draft,
        PrState::Closed,
        PrState::Merged,
    ] {
        let visible = matches!(state, PrState::Open | PrState::Draft);
        board.update(cx, |board, cx| {
            let account = board.data.get_mut("ann").unwrap();
            account.sections.get_mut(board.section).unwrap().items[0].state = state;
            account.details.values_mut().next().unwrap().item.state = state;
            board.rebuild(cx);
            assert_eq!(
                board
                    .item_commands(cx)
                    .iter()
                    .any(|command| command.label.as_ref() == "Close pull request…"),
                visible
            );
        });
        settle(cx);
        cx.update(|window, _| {
            assert_eq!(window.try_find(("gh-close", 3usize)).is_some(), visible);
            if visible {
                assert_eq!(
                    window.find(("gh-close", 3usize)).label(),
                    Some("Close pull request…")
                );
            }
        });
        cx.simulate_keystrokes("x");
        settle(cx);
        board.read_with(cx, |board, _| {
            assert_eq!(board.close_confirmation.is_some(), visible)
        });
        cx.simulate_keystrokes("escape");
        settle(cx);
    }
}

#[gpui_kit::test]
fn close_shortcut_works_in_diff_and_dialog_traps_tab_focus(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx);
    cx.simulate_keystrokes("d");
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.diff.is_some()));
    cx.simulate_keystrokes("x");
    settle(cx);
    board.read_with(cx, |board, _| assert!(board.close_confirmation.is_some()));
    cx.simulate_keystrokes("tab tab tab tab");
    settle(cx);
    board.update_in(cx, |board, window, cx| {
        let pending = board.close_confirmation.as_ref().unwrap();
        assert!(pending.focus.contains_focused(window, cx));
    });
    cx.simulate_keystrokes("escape");
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(board.diff.is_some());
        assert!(board.close_confirmation.is_none());
    });
}

#[gpui_kit::test]
fn in_flight_confirmation_blocks_repeat_submission_and_cancel(cx: &mut TestAppContext) {
    let (board, cx) = setup(cx);
    cx.simulate_keystrokes("x");
    settle(cx);
    board.update(cx, |board, cx| {
        board.writing = true;
        cx.notify();
    });
    settle(cx);
    cx.update(|window, _| {
        assert_eq!(window.find("github-close-submit").disabled(), Some(true));
        assert_eq!(window.find("github-close-cancel").disabled(), Some(true));
    });
    cx.simulate_keystrokes(subspace_framework::dashboard::secondary!("enter"));
    cx.simulate_keystrokes("escape");
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(board.writing);
        assert!(board.close_confirmation.as_ref().unwrap().error.is_none());
    });
}
