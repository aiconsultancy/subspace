//! A read made on no one's request never displaces a request made since
//! (SUB decision `a884e4c6`): a read again at the size page 0 claimed is
//! made only when it, what it replaces and every newer request fit one
//! ledger, and a replacement arriving has its window fitted to the room
//! left beside every newer request.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{SharedString, TestAppContext, VisualTestContext};
use serde_json::json;

use super::super::super::super::remove::settle;
use super::super::mounted;
use super::growth::{drawn_body, key, ledger, roomed};
use super::{answer, drawn, label, load, on, paged, refresh, running, served, tool};
use crate::view::inspector::activity::{
    ask_for, held_bytes, inline_bytes, latest_request, load_item,
};

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
    // The older version may be in parts too: only the newer one's bytes
    // tell the swap from what was shown staying. The page shows whole or,
    // within a smaller window, a piece at a time, and the newer body may
    // scroll it out of sight, so what is mounted of either is read.
    let part = mounted(&on("json-part"), cx) + &mounted(&on("body"), cx);
    assert!(
        part.contains("yyyy") && !part.contains("xxxx"),
        "the newer version shown: {part}"
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

/// Round 8 delta (P2 1), SUB decision `a884e4c6` (Deimos's case): with
/// 270,000 bytes of room, a result of 200,000 shown in parts, a
/// replacement declared 10,000 whose page 0 claims 90,000, and a newer
/// body of 65,000 asked for while it is read. Reading it again whole
/// would hold the shown result, the 90,000 coming in and the newer body
/// at once, more than the room: its page 0 is shown in parts instead,
/// within the room the newer body leaves.
#[gpui_kit::test]
fn a_grown_replacement_does_not_displace_a_newer_body(cx: &mut TestAppContext) {
    check_replacement_priority(cx, (270_000, 200_000), (10_000, 90_000), 65_000);
}

/// Round 8 delta (P2 1), SUB decision `a884e4c6`: a first read of `i3`
/// declared 10,000 whose page 0 claims 90,000, and a newer read of `i5`
/// in parts whose placeholder names it with 120,000 bytes, both under way
/// within 400,000 bytes of room. Read again whole, `i3` would take room
/// `i5`'s placeholder holds: the ledger counts a newer read's placeholder
/// beside the read, so `i3` shows page 0 in parts and `i5`'s read stays.
#[gpui_kit::test]
fn a_grown_read_does_not_displace_a_newer_read_and_its_placeholder(cx: &mut TestAppContext) {
    const ROOM: u64 = 400_000;
    let (board, served, cx) = served(cx, 6, &"y".repeat(90_000));
    let mut other = tool(6, &"z".repeat(250_000));
    other["id"] = json!("i5");
    let other = other.to_string();
    let bytes = other.len() as u64;
    served
        .others
        .lock()
        .unwrap()
        .insert("i5".to_owned(), (6, other));
    let mut items = paged(6, 10_000);
    items.as_array_mut().unwrap().push(json!({
        "id": "i5", "seq": 6, "startedAt": 1, "updatedAt": 2, "kind": "tool",
        "status": "completed", "callId": "call-5", "name": "n".repeat(120_000),
        "paged": {"pages": bytes.div_ceil(super::PAGE as u64), "bytes": bytes}
    }));
    refresh(&board, cx);
    answer(&board, Ok(running(items)), cx);
    roomed(&board, ROOM, cx);

    cx.update(|window, cx| {
        window.click(SharedString::from(on("load")), cx);
        // Its card is far below its name, so it is read as its action does.
        board.update(cx, |board, cx| load_item(board, ("t-now", "i5", bytes), cx));
    });
    let latest = |cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            latest_request(board.inspector_activity.as_ref().unwrap())
        })
    };
    assert_eq!(latest(cx), Some(key("i5")), "newer read requested");
    assert_eq!(ledger(&board, cx).0, vec![key("i3"), key("i5")], "both fit");
    settle(cx);
    assert_eq!(latest(cx), Some(key("i5")));
    assert_eq!(
        ledger(&board, cx).0,
        vec![key("i3"), key("i5")],
        "newer read displaced; gateway saw {:?}",
        served.asked()
    );
    assert!(
        drawn(&on("parts"), cx).starts_with("Too large to hold whole"),
        "page 0 shown in parts"
    );
    let (_, used) = ledger(&board, cx);
    assert!(used <= ROOM, "{used} past the room");
}

/// Round 9 thermo (P3 1), SUB decision `a884e4c6` and rule `469f1320`:
/// with 66,000 bytes of room, a small result shown whole, a replacement
/// declared 9,000 whose page 0 claims 90,000, and a newer reply of 3,000
/// asked for while it is read. Page 0 and a part of its window do not fit
/// the room the newer reply leaves, so the swap is refused: what was shown
/// stays, charged as before, the newer reply stays the latest request, and
/// beside **Load the latest**, offered again, a caption says why. Loading
/// it again, now the latest request, shows it.
#[gpui_kit::test]
fn a_refused_replacement_keeps_what_is_shown_and_says_so(cx: &mut TestAppContext) {
    const ROOM: u64 = 66_000;
    let (board, served, cx) = served(cx, 6, &"x".repeat(1_000));
    load(cx);
    served.serve(7, tool(7, &"y".repeat(90_000)));
    let mut items = paged(7, 9_000);
    items.as_array_mut().unwrap().push(json!({
        "id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2,
        "kind": "text", "status": "completed", "text": "a".repeat(3_000)
    }));
    refresh(&board, cx);
    answer(&board, Ok(running(items)), cx);
    roomed(&board, ROOM, cx);
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Load the latest"));
    let charge = |cx: &mut VisualTestContext| {
        board.read_with(cx, |board, _| {
            let selection = board.inspector_activity.as_ref().unwrap();
            held_bytes(selection) - inline_bytes(selection)
        })
    };
    let shown = charge(cx);

    cx.update(|window, cx| {
        window.click(SharedString::from(on("load")), cx);
        board.update(cx, |board, cx| {
            ask_for(board.inspector_activity.as_mut().unwrap(), "t-now", "i1");
            cx.notify();
        });
    });
    assert_eq!(ledger(&board, cx).0, vec![key("i3")], "both fit during I/O");
    settle(cx);
    assert!(
        drawn(&on("out:0"), cx).contains("xxxx"),
        "what was shown stays; gateway saw {:?}",
        served.asked()
    );
    assert!(drawn(&on("parts"), cx).is_empty(), "page 0 not swapped in");
    assert_eq!(charge(cx), shown, "charged as before");
    let latest = board.read_with(cx, |board, _| {
        latest_request(board.inspector_activity.as_ref().unwrap())
    });
    assert_eq!(latest, Some(key("i1")), "the newer reply stays the latest");
    assert_eq!(
        drawn(&on("deferred"), cx),
        "The newer version did not fit beside a later request: load it again to show it."
    );
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Load the latest"));
    let (_, used) = ledger(&board, cx);
    assert!(used <= ROOM, "{used} past the room");

    load(cx);
    assert!(drawn(&on("deferred"), cx).is_empty());
    assert!(
        drawn(&on("parts"), cx).starts_with("Too large to hold whole"),
        "the newer version shown in parts"
    );
}
