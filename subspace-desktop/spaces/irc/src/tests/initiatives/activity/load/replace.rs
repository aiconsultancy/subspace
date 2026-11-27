//! A read replacing what is shown never displaces a request made since
//! (SUB decision `a884e4c6`): once it arrives, its window is fitted to the
//! room left beside every newer request.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{SharedString, TestAppContext, VisualTestContext};
use serde_json::json;

use super::super::super::super::remove::settle;
use super::growth::{drawn_body, key, ledger, roomed};
use super::{answer, drawn, load, on, paged, refresh, running, served, tool};
use crate::view::inspector::activity::{ask_for, latest_request};

/// `i3` read whole or in parts with `old` bytes of output within `room`,
/// then named at a newer version, declared `declared` bytes and served
/// with `actual`; **Load the latest** clicked, then a newer inline body of
/// `newer_body` bytes asked for while that read is under way. Once it
/// arrives the newer body, and its request, stay.
fn check_replacement_priority(
    cx: &mut TestAppContext,
    (room, old): (u64, usize),
    (declared, actual): (usize, usize),
    newer_body: usize,
) {
    let (board, served, cx) = served(cx, 6, &"x".repeat(old));
    roomed(&board, room, cx);
    load(cx);
    assert_eq!(ledger(&board, cx).0, vec![key("i3")]);
    served.serve(7, tool(7, &"y".repeat(actual)));
    let mut items = paged(7, declared);
    items.as_array_mut().unwrap().push(json!({
        "id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2,
        "kind": "text", "status": "completed", "text": "a".repeat(newer_body)
    }));
    refresh(&board, cx);
    answer(&board, Ok(running(items)), cx);
    roomed(&board, room, cx);

    cx.update(|window, cx| {
        window.click(SharedString::from(on("load")), cx);
        board.update(cx, |board, cx| {
            ask_for(board.inspector_activity.as_mut().unwrap(), "t-now", "i1");
            cx.notify();
        });
    });
    let latest = |cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            latest_request(board.inspector_activity.as_ref().unwrap())
        })
    };
    assert_eq!(latest(cx), Some(key("i1")), "newer body requested");
    assert_eq!(ledger(&board, cx).0, vec![key("i3")], "both fit during I/O");
    settle(cx);
    assert_eq!(
        latest(cx),
        Some(key("i1")),
        "newer body displaced once the read arrived; gateway saw {:?}",
        served.asked()
    );
    assert_eq!(
        ledger(&board, cx).0,
        vec![key("i3")],
        "the replacement kept"
    );
    assert!(
        drawn(&on("parts"), cx).starts_with("Too large to hold whole"),
        "and shown"
    );
    assert!(drawn_body(&board, cx) >= newer_body as u64);
    let (_, used) = ledger(&board, cx);
    assert!(used <= room, "{used} past the room");
}

/// Round 8 delta (P2 2), SUB decision `a884e4c6` (Deimos's case): with
/// 255,000 bytes of room, a small result shown whole, a replacement too
/// large to read whole, granted a page and a window of a part, and a newer
/// body of 170,000 asked for while its page comes in, beside which the
/// shown result and that page fit. Once the page arrives its window is
/// sized to the 85,000 bytes the newer body leaves, not to the grant made
/// before it was asked for.
#[gpui_kit::test]
fn publishing_a_replacement_part_does_not_displace_a_newer_body(cx: &mut TestAppContext) {
    check_replacement_priority(cx, (255_000, 5_000), (200_000, 200_000), 170_000);
}
