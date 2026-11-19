//! A folded group's first page against the hub: a page that came back
//! empty ends its group only for the list it was read for, and only a
//! read the hub queued counts. Every replacement list reads it again.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::dashboard::Dashboard as _;

use super::offline::{DONE, Offline, board_with_hub, fixture_hub, offline, settle_board, wait_for};
use super::press;
use crate::{Issues, lines, link};

#[gpui_kit::test]
fn a_cancelled_first_page_is_read_again_after_an_offline_reload(cx: &mut TestAppContext) {
    let offline = offline();
    offline.switches.hold_pages.store(true, Ordering::Relaxed);
    let (board, cx) = board_with_hub(&offline, cx);
    settle_board(&board, cx, |board| board.listing.content.is_some());
    unfold_done(&board, cx);
    wait_for(|| done_reads(&offline) == 1);
    offline
        .switches
        .offline_lists
        .store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| board.reload_list(cx));
    let more = release_page_after_offline_reload(&board, &offline, cx);
    assert_ne!(
        more,
        crate::listing::More::None,
        "cancellation is not an empty completion"
    );
    settle_board(&board, cx, |board| done_rows(board) == Some(1));
    assert_eq!(done_reads(&offline), 2, "the cancelled page was retried");
}

#[gpui_kit::test]
fn unfolding_during_a_failed_reload_does_not_complete_an_unqueued_page(cx: &mut TestAppContext) {
    let offline = offline();
    let (board, cx) = board_with_hub(&offline, cx);
    settle_board(&board, cx, |board| board.listing.content.is_some());
    offline.switches.hold_lists.store(true, Ordering::Relaxed);
    offline.switches.hold_pages.store(true, Ordering::Relaxed);
    offline
        .switches
        .offline_lists
        .store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| board.reload_list(cx));
    unfold_done(&board, cx);
    board.update(cx, |board, cx| board.poll_hub(cx));
    assert_eq!(done_reads(&offline), 0, "the reload refused a page read");
    offline.switches.hold_lists.store(false, Ordering::Relaxed);
    let more = release_page_after_offline_reload(&board, &offline, cx);
    assert_ne!(more, crate::listing::More::None, "no page completed");
    settle_board(&board, cx, |board| done_rows(board) == Some(1));
    assert_eq!(done_reads(&offline), 1, "the open group can still be read");
}

/// Observe the failed Reload before letting its waiting page complete.
fn release_page_after_offline_reload(
    board: &Entity<Issues>,
    offline: &Offline,
    cx: &mut VisualTestContext,
) -> crate::listing::More {
    settle_board(board, cx, |board| board.listing.offline().is_some());
    let more = cx.update(|_, cx| board.read(cx).listing.content.as_ref().unwrap().groups[1].more);
    offline.switches.hold_pages.store(false, Ordering::Relaxed);
    more
}

fn done_reads(offline: &Offline) -> usize {
    offline
        .mock
        .requests()
        .iter()
        .filter(|request| request.path.contains(DONE))
        .count()
}

/// The Done group's rows, once the board has its content.
fn done_rows(board: &Issues) -> Option<usize> {
    Some(board.listing.content.as_ref()?.groups.get(1)?.rows.len())
}

/// Unfolds Done with Enter, as the list does.
fn unfold_done(board: &Entity<Issues>, cx: &mut VisualTestContext) {
    board.update(cx, |board, _| board.list.select(lines::group_key(DONE)));
    press(cx, "enter");
}

fn grouped_reads(offline: &Offline) -> usize {
    let requests = offline.mock.requests();
    requests
        .iter()
        .filter(|request| {
            request.path.contains("issues/groups") && !request.path.contains("limitPerGroup=0")
        })
        .count()
}

/// A completed empty page, whose marker must be forgotten on replacement.
fn done_empty<'a>(
    offline: &Offline,
    cx: &'a mut TestAppContext,
) -> (Entity<Issues>, &'a mut VisualTestContext) {
    offline.switches.empty_pages.store(true, Ordering::Relaxed);
    let (board, cx) = board_with_hub(offline, cx);
    settle_board(&board, cx, |board| board.listing.content.is_some());
    unfold_done(&board, cx);
    settle_board(&board, cx, |board| {
        board.listing.content.as_ref().unwrap().groups[1].more == crate::listing::More::None
    });
    assert_eq!(cx.update(|_, cx| done_rows(board.read(cx))), Some(0));
    offline.switches.empty_pages.store(false, Ordering::Relaxed);
    (board, cx)
}

#[gpui_kit::test]
fn returning_to_a_replaced_list_reads_dones_first_page_again(cx: &mut TestAppContext) {
    let offline = offline();
    let (board, cx) = done_empty(&offline, cx);
    cx.update(|window, cx| board.update(cx, |board, cx| board.deactivate(window, cx)));
    let hub = cx.update(|_, cx| link::current(cx).unwrap().hub);
    let installation = hub.snapshot().list_id();
    let reads = done_reads(&offline);
    // No timestamp is stale too, including on systems whose clock cannot
    // represent an instant a minute ago.
    hub.set_list_updated_at_for_test(Instant::now().checked_sub(Duration::from_secs(61)));
    offline.switches.hold_lists.store(true, Ordering::Relaxed);
    cx.update(|window, cx| board.update(cx, |board, cx| board.activate(window, cx)));
    assert!(
        hub.snapshot().list_loading(),
        "the unchanged query took the stale path"
    );
    assert!(hub.snapshot().list().is_some(), "no cleared-list interval");
    offline.switches.hold_lists.store(false, Ordering::Relaxed);
    wait_for(|| hub.snapshot().list_id() != installation);
    settle_board(&board, cx, |board| done_rows(board) == Some(1));
    assert_eq!(done_reads(&offline), reads + 1);
}

#[gpui_kit::test]
fn replacing_the_hub_forgets_empty_pages_even_when_list_ids_match(cx: &mut TestAppContext) {
    let first = offline();
    let (board, cx) = done_empty(&first, cx);
    let old = cx.update(|_, cx| link::current(cx).unwrap().hub);
    let second = offline();
    let (hub, address) = fixture_hub(&second);
    let query = cx.update(|_, cx| board.read(cx).choice().query());
    hub.activate(query).unwrap();
    wait_for(|| hub.snapshot().list().is_some());
    assert_eq!(hub.snapshot().list_id(), old.snapshot().list_id());
    assert_eq!(hub.snapshot().generation(), old.snapshot().generation());
    second.switches.hold_pages.store(true, Ordering::Relaxed);
    cx.update(|_, cx| link::install(cx, hub, Some(address)));
    cx.run_until_parked();
    wait_for(|| done_reads(&second) == 1);
    second.switches.hold_pages.store(false, Ordering::Relaxed);
    let deadline = Instant::now() + Duration::from_secs(10);
    while cx.update(|_, cx| done_rows(board.read(cx))) != Some(1) {
        assert!(
            Instant::now() < deadline,
            "the new responder didn't refresh the board"
        );
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        done_reads(&second),
        1,
        "the new hub's Done page is independent"
    );
}

#[gpui_kit::test]
fn cursor_recovery_reads_dones_first_page_again(cx: &mut TestAppContext) {
    let offline = offline();
    let (board, cx) = done_empty(&offline, cx);
    let lists = grouped_reads(&offline);
    let pages = done_reads(&offline);
    let hub = cx.update(|_, cx| link::current(cx).unwrap().hub);
    let installation = hub.snapshot().list_id();
    offline
        .switches
        .refuse_cursors
        .store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| board.read_more("state-started", cx));
    settle_board(&board, cx, |board| {
        hub.snapshot().list_id() != installation
            && done_reads(&offline) > pages
            && done_rows(board) == Some(1)
    });
    assert_eq!(
        grouped_reads(&offline),
        lists + 1,
        "a real replacement list"
    );
    assert_eq!(
        done_reads(&offline),
        pages + 1,
        "Done was read after recovery"
    );
}

#[gpui_kit::test]
fn reload_reads_a_once_empty_group_again(cx: &mut TestAppContext) {
    let offline = offline();
    let (board, cx) = done_empty(&offline, cx);
    offline.switches.hold_lists.store(true, Ordering::Relaxed);
    board.update(cx, |board, cx| board.reload_list(cx));
    board.update(cx, |board, cx| board.poll_hub(cx));
    offline.switches.hold_lists.store(false, Ordering::Relaxed);
    settle_board(&board, cx, |board| done_rows(board) == Some(1));
    cx.update(|_, cx| assert!(!board.read(cx).busy(cx)));
}
