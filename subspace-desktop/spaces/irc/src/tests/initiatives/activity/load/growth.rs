//! A whole read's optional growth (SUB decisions `feb0d839` and `4311830a`
//! items 6 and 7): its window grows past its allowance only into room
//! nothing else holds, no other read and no body asked for, so publishing
//! it never lets anything go, and it is the first thing given back when a
//! newer request wants room.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

use super::super::super::super::remove::settle;
use super::super::mounted;
use super::{
    Irc, answer, drawn, held_bytes, load, loaded_keys, on, paged, refresh, running, served,
};
use crate::view::inspector::activity::{ask_for, drawn_bytes, inline_bytes, set_room};

/// The room the view leaves beside its inline content in Phobos's case.
const ROOM: u64 = 255_000;

/// `room` bytes left beside the inline content, drawn again.
fn roomed(board: &Entity<Irc>, room: u64, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        set_room(board.inspector_activity.as_mut().unwrap(), room);
        cx.notify();
    });
    settle(cx);
}

/// Item `i4` in full: a reply of about 30,000 bytes of JSON.
fn second() -> Value {
    json!({"id": "i4", "seq": 3, "startedAt": 1, "updatedAt": 2, "kind": "text",
           "status": "completed", "text": "b".repeat(29_800)})
}

/// The snapshot's word for `i4`: too large to send inline.
fn second_paged(bytes: usize) -> Value {
    json!({"id": "i4", "seq": 3, "startedAt": 1, "updatedAt": 2, "kind": "text",
           "status": "completed", "paged": {"pages": 1, "bytes": bytes}})
}

/// The snapshot: `i3` of `bytes` too large to send inline, `i4` too,
/// before it when `second_first`, and a reply of 40,000 bytes sent
/// inline, last. A load action is clicked while nothing drawn whole above
/// it pushes it out of sight.
fn snapshot(bytes: usize, (second, second_first): (usize, bool)) -> Value {
    let mut items = paged(6, bytes);
    let items_mut = items.as_array_mut().unwrap();
    let at = match second_first {
        true => items_mut
            .iter()
            .position(|item| item["id"] == "i3")
            .unwrap(),
        false => items_mut.len(),
    };
    items_mut.insert(at, second_paged(second));
    items_mut.push(json!({"id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2,
                          "kind": "text", "status": "completed", "text": "a".repeat(40_000)}));
    items
}

/// The turn view with `i3`, whose markers draw far more than its JSON,
/// and `i4` beside it, before it when `second_first`, both too large to
/// send inline, `room` bytes left.
fn markers(
    cx: &mut TestAppContext,
    (room, second_first): (u64, bool),
) -> (Entity<Irc>, &mut VisualTestContext) {
    // Each marker is 3 bytes of JSON and draws 12.
    let (board, served, cx) = served(cx, 6, &"\u{202E}".repeat(12_000));
    let bytes = served.item.lock().unwrap().1.len();
    let serving = (3, second().to_string());
    served
        .others
        .lock()
        .unwrap()
        .insert("i4".to_owned(), serving);
    let second = second().to_string().len();
    refresh(&board, cx);
    answer(
        &board,
        Ok(running(snapshot(bytes, (second, second_first)))),
        cx,
    );
    roomed(&board, room, cx);
    (board, cx)
}

/// Reads `i4`, as **Load in full** does.
fn load_second(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.click(SharedString::from("irc-activity:t-now:i4:load"), cx);
    });
    settle(cx);
}

/// The items read, and what they and the open turns' bodies count beside
/// the inline content.
fn ledger(board: &Entity<Irc>, cx: &mut VisualTestContext) -> (Vec<(String, String)>, u64) {
    board.read_with(cx, |board, _| {
        let selection = board.inspector_activity.as_ref().unwrap();
        let used = held_bytes(selection) + drawn_bytes(selection) - inline_bytes(selection);
        (loaded_keys(selection), used)
    })
}

/// What the open turns' bodies draw: the body asked for among them.
fn drawn_body(board: &Entity<Irc>, cx: &mut VisualTestContext) -> u64 {
    board.read_with(cx, |board, _| {
        drawn_bytes(board.inspector_activity.as_ref().unwrap())
    })
}

fn key(item: &str) -> (String, String) {
    ("t-now".to_owned(), item.to_owned())
}

/// Round 6 thermo (P2 5), SUB decisions `feb0d839` and `4311830a` item 6:
/// with 255,000 bytes of room, a second read of 30,000 bytes and a body of
/// 40,000 asked for inline after it, a whole read whose markers draw far
/// more than its JSON is published without letting either go: its window
/// grows only into room nothing holds, and otherwise draws in parts within
/// its allowance. All three fit.
#[gpui_kit::test]
fn publishing_a_whole_read_never_lets_go_of_another_to_grow(cx: &mut TestAppContext) {
    let (board, cx) = markers(cx, (ROOM, false));
    load_second(cx);
    board.update(cx, |board, cx| {
        ask_for(board.inspector_activity.as_mut().unwrap(), "t-now", "i1");
        cx.notify();
    });
    settle(cx);
    load(cx);

    let (keys, used) = ledger(&board, cx);
    assert_eq!(keys, vec![key("i3"), key("i4")], "neither read let go");
    assert!(drawn_body(&board, cx) >= 40_000, "nor the body");
    assert!(
        drawn(&on("pieced"), cx).starts_with("Too large to draw whole"),
        "drawn in parts within its allowance, not grown"
    );
    assert!(used <= ROOM, "{used} past the room");
}

/// Round 6 thermo (P3), SUB decisions `feb0d839` and `4311830a` item 7: a
/// whole read that grew into room nothing held gives that growth back
/// when a newer read is admitted, and is not let go: both fit within
/// their allowances, which the room holds.
#[gpui_kit::test]
fn growth_is_given_back_to_a_newer_read_before_anything_is_let_go(cx: &mut TestAppContext) {
    let room = 210_000;
    let (board, cx) = markers(cx, (room, true));
    load(cx);
    assert!(mounted(&on("pieced"), cx).is_empty(), "grown: drawn whole");
    // Its allowance is its JSON and a page beside it, about 102,000.
    let (_, grown) = ledger(&board, cx);
    assert!(grown > room / 2, "{grown}: past its allowance");

    load_second(cx);
    let (keys, used) = ledger(&board, cx);
    assert_eq!(keys, vec![key("i3"), key("i4")], "the grown read kept");
    // Below `i4` drawn whole: mounted, out of sight.
    assert!(
        mounted(&on("pieced"), cx).starts_with("Too large to draw whole"),
        "its growth given back"
    );
    assert!(used <= room, "{used} past the room");
}
