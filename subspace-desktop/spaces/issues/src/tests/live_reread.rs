//! A live hint's list read against the hub: rows paged in below the first
//! page are read again from fresh cursors and stay shown, and so does the
//! selection on one of them.

use super::offline::{board_with_hub, offline, settle_board, wait_for};
use super::selected;
use crate::{Issues, lines, link};

fn rows(board: &gpui_kit::Entity<Issues>, cx: &mut gpui_kit::VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        let board = board.read(cx);
        let content = board.listing.content.as_ref().unwrap();
        let group = content
            .groups
            .iter()
            .find(|group| group.key == "state-started")
            .unwrap();
        group.rows.iter().map(|row| row.fields.id.clone()).collect()
    })
}

#[gpui_kit::test]
fn a_reread_keeps_paged_rows_and_the_selection(cx: &mut gpui_kit::TestAppContext) {
    let offline = offline();
    let (board, cx) = board_with_hub(&offline, cx);
    settle_board(&board, cx, |board| board.listing.content.is_some());
    board.update(cx, |board, cx| board.read_more("state-started", cx));
    settle_board(&board, cx, |board| {
        board
            .listing
            .content
            .as_ref()
            .and_then(|content| content.groups.iter().find(|g| g.key == "state-started"))
            .is_some_and(|group| group.rows.len() == 2)
    });
    let key = lines::issue_key("2");
    board.update(cx, |board, _| board.list.select(key.clone()));
    assert_eq!(selected(&board, cx).as_deref(), Some(key.as_ref()));
    let hub = cx.update(|_, cx| link::current(cx).unwrap().hub);
    let installation = hub.snapshot().list_id();
    hub.reread_list_for_test();
    wait_for(|| hub.snapshot().list_id() != installation);
    settle_board(&board, cx, |board| {
        board.listing.content.is_some() && board.list.selected().is_some()
    });
    assert_eq!(rows(&board, cx), ["1", "2"], "the paged row stays shown");
    assert_eq!(selected(&board, cx).as_deref(), Some(key.as_ref()));
}
