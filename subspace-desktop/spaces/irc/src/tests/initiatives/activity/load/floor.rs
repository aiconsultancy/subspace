//! One ledger for every read and body (SUB decision `4311830a` item 1): a
//! read's window is sized to the positive room left, a refresh keeps the
//! latest request within the floor's steady allowance, and the floor
//! belongs to the latest request, never to a body asked for before it,
//! and to one presentation of it: a read in parts keeps it, never its
//! placeholder too (SUB decision `084f5236` item 1).

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use serde_json::json;

use super::super::super::super::remove::settle;
use super::{
    Irc, PAGE, answer, drawn, held_bytes, label, load, loaded_keys, on, paged, refresh, running,
    served, served_json,
};
use crate::view::inspector::activity::{drawn_bytes, set_room};

/// The floor's steady allowance for a read: a part and its drawn window.
const FLOOR: u64 = 2 * PAGE as u64;

/// `room` bytes left beside the inline content, drawn again.
fn roomed(board: &Entity<Irc>, room: u64, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        set_room(board.inspector_activity.as_mut().unwrap(), room);
        cx.notify();
    });
    settle(cx);
}

/// The bytes the view holds of the captain's activity.
fn held(board: &Entity<Irc>, cx: &mut VisualTestContext) -> u64 {
    board.read_with(cx, |board, _| {
        held_bytes(board.inspector_activity.as_ref().unwrap())
    })
}

/// Round 6 thermo (P1), SUB decision `4311830a` item 1: a read in parts
/// with less room than a page and a whole part's window sizes its window
/// to the room left, never to the floor's part, while it is read and once
/// its part arrives; the part still draws, in parts of its own.
#[gpui_kit::test]
fn a_part_read_sizes_its_window_to_the_room_left(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, &"x".repeat(200_000));
    let room = 70_000;
    roomed(&board, room, cx);
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Load in parts"));
    let inline = held(&board, cx);
    cx.update(|window, cx| window.click(SharedString::from(on("load")), cx));
    assert!(held(&board, cx) - inline <= room, "while read");
    settle(cx);
    let read = held(&board, cx) - inline;
    assert!(read <= room, "once read: {read}");
    let (json, _) = served_json(&served);
    assert!(drawn(&on("pieced"), cx).starts_with("Too large to draw whole"));
    let shown = drawn(&on("body"), cx);
    assert!(!shown.is_empty() && json.starts_with(&shown), "{shown}");
}

/// Round 6 thermo (P1), SUB decision `4311830a` item 1: a refresh that
/// leaves no room keeps the latest request's read within the floor's
/// steady allowance, a part and its window, even when it was settled
/// whole and larger: its window is drawn in parts within the floor.
#[gpui_kit::test]
fn a_refresh_at_the_floor_keeps_a_read_within_a_part_and_its_window(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, &format!("{}END", "x".repeat(80_000)));
    let inline = held(&board, cx);
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("END"), "read whole");
    assert!(held(&board, cx) - inline > FLOOR, "settled past the floor");
    roomed(&board, 0, cx);
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(6, bytes))), cx);
    let read = held(&board, cx) - inline;
    assert!(read <= FLOOR, "{read} past the floor's {FLOOR}");
    let keys = board.read_with(cx, |board, _| {
        loaded_keys(board.inspector_activity.as_ref().unwrap())
    });
    assert_eq!(keys, vec![("t-now".to_owned(), "i3".to_owned())]);
    assert!(drawn(&on("pieced"), cx).starts_with("Too large to draw whole"));
}

/// Round 6 thermo (P1), SUB decision `4311830a` item 1: the floor belongs
/// to the latest request. When that is an item read, which draws within
/// its own allowance, a body asked for before it never passes the room in
/// its place.
#[gpui_kit::test]
fn the_floor_never_passes_to_a_body_asked_for_before_the_latest_read(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, "short");
    let bytes = served.item.lock().unwrap().1.len();
    let snapshot = || {
        let mut items = paged(6, bytes);
        let reply = json!({"id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2,
                           "kind": "text", "status": "completed", "text": "a".repeat(200)});
        items.as_array_mut().unwrap().insert(0, reply);
        running(items)
    };
    refresh(&board, cx);
    answer(&board, Ok(snapshot()), cx);
    let body = "irc-activity:t-now:i1";
    roomed(&board, 0, cx);
    assert!(!drawn(&format!("{body}:held-back"), cx).is_empty());
    cx.update(|window, cx| window.click(SharedString::from(format!("{body}:show")), cx));
    settle(cx);
    assert!(
        drawn(&format!("{body}:held-back"), cx).is_empty(),
        "asked for"
    );

    roomed(&board, 1 << 30, cx);
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("short"), "read last");
    roomed(&board, 0, cx);
    refresh(&board, cx);
    answer(&board, Ok(snapshot()), cx);
    assert!(drawn(&on("out:0"), cx).ends_with("short"), "the floor's");
    assert!(
        !drawn(&format!("{body}:held-back"), cx).is_empty(),
        "an earlier body never takes the floor"
    );
}

/// The bytes the bodies the open turns draw hold.
fn planned(board: &Entity<Irc>, cx: &mut VisualTestContext) -> u64 {
    board.read_with(cx, |board, _| {
        drawn_bytes(board.inspector_activity.as_ref().unwrap())
    })
}

fn click(part: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(SharedString::from(on(part)), cx));
    settle(cx);
}

/// `i3` of `bytes` named at a newer version, its placeholder naming
/// `name`: a body of its own, planned like any other.
fn named(board: &Entity<Irc>, (bytes, name): (usize, &str), cx: &mut VisualTestContext) {
    let mut items = paged(7, bytes);
    items[0]["name"] = json!(name);
    refresh(board, cx);
    answer(board, Ok(running(items)), cx);
}

/// Round 7 delta (P1), SUB decision `084f5236` item 1 (Deimos's fixture):
/// with no room left, an item read in parts is the one presentation the
/// floor charges: its part and window. What its placeholder names takes no
/// floor of its own, so the total stays within a part and its window.
#[gpui_kit::test]
fn at_no_room_a_part_read_is_charged_its_part_and_window_alone(cx: &mut TestAppContext) {
    let (board, _served, cx) = served(cx, 6, &"x".repeat(200_000));
    roomed(&board, 0, cx);
    let inline = held(&board, cx);
    load(cx);
    assert!(drawn(&on("parts"), cx).starts_with("Too large to hold whole"));
    let charged = held(&board, cx) - inline;
    assert_eq!(charged, FLOOR, "a part and its window");
    let total = charged + planned(&board, cx);
    assert!(
        total <= FLOOR,
        "{total} past the floor's {FLOOR}: a second floor"
    );
    assert!(
        !drawn(&on("held-back"), cx).is_empty(),
        "its placeholder held back"
    );
}

/// Round 7 delta (P1), SUB decision `084f5236` item 1: an item read in
/// parts whose window draws in parts, beside its placeholder drawn in
/// parts within the room left: each has its own pager, and each pager
/// moves only its own body.
#[gpui_kit::test]
fn a_pieced_placeholder_and_its_read_each_move_their_own_parts(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, &"x".repeat(200_000));
    let (json, _) = served_json(&served);
    // Room for a page and a window of about 25,000 bytes.
    roomed(&board, PAGE as u64 + 25_000, cx);
    let inline = held(&board, cx);
    load(cx);
    let read = held(&board, cx) - inline;
    // A newer version named with a long name, and too little room left
    // beside the read to draw it whole: in parts of 452 bytes. The read
    // stays shown until the newer version is asked for.
    named(&board, (json.len(), &"N".repeat(10_000)), cx);
    roomed(&board, read + 5_000, cx);
    assert!(drawn(&on("pieced"), cx).starts_with("Too large to draw whole"));
    assert!(drawn(&on("named:pieced"), cx).starts_with("Too large to draw whole"));
    assert!(drawn(&on("piece"), cx).starts_with("Part 1 of "));
    assert!(drawn(&on("named:piece"), cx).starts_with("Part 1 of "));
    let (page, first) = (drawn(&on("body"), cx), drawn(&on("named:body"), cx));
    assert!(
        json.starts_with(&page) && first.starts_with("Name"),
        "{first}"
    );

    click("named:piece-next", cx);
    assert!(drawn(&on("named:piece"), cx).starts_with("Part 2 of "));
    assert_ne!(drawn(&on("named:body"), cx), first);
    assert!(
        drawn(&on("piece"), cx).starts_with("Part 1 of "),
        "the read's stays"
    );
    assert_eq!(drawn(&on("body"), cx), page);

    click("piece-next", cx);
    assert!(drawn(&on("piece"), cx).starts_with("Part 2 of "));
    assert_ne!(drawn(&on("body"), cx), page);
    assert!(
        drawn(&on("named:piece"), cx).starts_with("Part 2 of "),
        "its own stays"
    );
}
