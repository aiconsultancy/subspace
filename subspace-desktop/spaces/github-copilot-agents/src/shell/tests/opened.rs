//! The opened task's tab: opening it, going back to the list, closing it,
//! and what the list's controls do while it shows.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use subspace_framework::dashboard::list;

use super::{TASK_KEY, board, drawn, settle, start};
use crate::CopilotAgents;
use crate::inspector::OPEN_ID;
use crate::item::ITEM_ID;
use crate::keys::Picking;
use crate::shell::BACK_ID;
use crate::ui::button_id;

#[gpui_kit::test]
fn enter_or_a_double_click_opens_the_row_as_a_tab_and_back_returns_to_the_list(
    cx: &mut TestAppContext,
) {
    start(cx);
    let (board, cx) = cx.add_window_view(board);
    board.update(cx, |board, cx| {
        list::update(board, cx, |list| list.select(TASK_KEY.into()));
        // What Enter and a double click do.
        list::update(board, cx, |list| {
            list.activate();
        });
        assert_eq!(board.item.showing(), Some(TASK_KEY));
        assert!(
            board.list.open_row().is_none(),
            "the shared detail panel stays shut"
        );
    });
    settle(cx);
    assert!(drawn(cx, ITEM_ID), "the task is the pane's content");
    assert!(drawn(cx, "github-copilot-agents-item-reading"));
    assert!(drawn(cx, "workspace-tab-strip"));
    assert!(drawn(cx, BACK_ID));
    assert!(
        drawn(cx, "github-copilot-agents-peek-head"),
        "the inspector shows the open task"
    );
    assert!(!drawn(cx, OPEN_ID), "an open task offers no Open");

    cx.update(|window, cx| window.click(BACK_ID, cx));
    settle(cx);
    assert!(!drawn(cx, ITEM_ID));
    board.read_with(cx, |board, _| {
        assert!(board.item.showing().is_none());
        assert_eq!(
            board.item.key.as_deref(),
            Some(TASK_KEY),
            "Back leaves the tab open"
        );
    });
}

#[gpui_kit::test]
fn open_in_the_inspector_opens_the_selection_and_closing_the_tab_shows_the_list(
    cx: &mut TestAppContext,
) {
    start(cx);
    let (board, cx) = cx.add_window_view(board);
    board.update(cx, |board, cx| {
        list::update(board, cx, |list| list.select(TASK_KEY.into()));
    });
    settle(cx);
    cx.update(|window, cx| window.click(OPEN_ID, cx));
    settle(cx);
    assert!(drawn(cx, ITEM_ID));

    board.update(cx, |board, cx| board.close_item(cx));
    settle(cx);
    assert!(!drawn(cx, ITEM_ID));
    board.read_with(cx, |board, _| {
        assert!(board.item.key.is_none(), "the tab is gone");
    });
}

/// Opens the fixture task as a tab and shows it, as Enter does.
fn open_task(board: &gpui_kit::Entity<CopilotAgents>, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        list::update(board, cx, |list| list.select(TASK_KEY.into()));
        list::update(board, cx, |list| {
            list.activate();
        });
    });
    settle(cx);
}

#[gpui_kit::test]
fn an_opened_task_that_leaves_the_list_keeps_a_tab_that_closes(cx: &mut TestAppContext) {
    start(cx);
    let (board, cx) = cx.add_window_view(board);
    open_task(&board, cx);
    board.update(cx, |board, cx| {
        board.tasks.clear();
        board.rebuild(cx);
        assert!(board.row(TASK_KEY).is_none(), "the row has gone");
        assert_eq!(board.item.showing(), Some(TASK_KEY));
    });
    settle(cx);
    assert!(drawn(cx, ITEM_ID), "the page says the task is gone");
    cx.update(|window, cx| {
        window
            .within("github-copilot-agents-tabs")
            .within(1usize)
            .click("close-tab", cx);
    });
    settle(cx);
    assert!(!drawn(cx, ITEM_ID));
    board.read_with(cx, |board, _| {
        assert!(board.item.key.is_none(), "its tab closed it");
    });
}

#[gpui_kit::test]
fn a_filter_key_on_an_opened_task_shows_the_list_and_opens_the_menu_there(cx: &mut TestAppContext) {
    start(cx);
    let (board, cx) = cx.add_window_view(board);
    open_task(&board, cx);
    assert!(drawn(cx, ITEM_ID));
    board.update_in(cx, |board, window, cx| {
        board.open_filter_menu(Picking::Kind, window, cx);
        assert!(board.menu.is_some(), "the kind menu is open");
    });
    settle(cx);
    board.read_with(cx, |board, _| {
        assert!(board.item.showing().is_none(), "the list shows");
        assert_eq!(
            board.item.key.as_deref(),
            Some(TASK_KEY),
            "the task's tab stays open"
        );
    });
    assert!(!drawn(cx, ITEM_ID));
    assert!(
        drawn(cx, button_id(Picking::Kind)),
        "the menu hangs from a drawn button"
    );
}
