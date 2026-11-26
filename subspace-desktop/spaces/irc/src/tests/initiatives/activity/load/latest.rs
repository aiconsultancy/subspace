//! **Load the latest** (SUB decision `4311830a` item 2): what was shown
//! stays shown and charged until the newer version arrives, and stays
//! beside the error and a retry when it fails; an answer never changes
//! which request is the latest.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use serde_json::json;

use super::super::super::super::remove::settle;
use super::{
    Irc, answer, drawn, held_bytes, label, load, on, paged, refresh, running, served, tool,
};
use crate::view::inspector::activity::{ask_for, latest_request};

/// The bytes the view holds of the captain's activity.
fn held(board: &Entity<Irc>, cx: &mut VisualTestContext) -> u64 {
    board.read_with(cx, |board, _| {
        held_bytes(board.inspector_activity.as_ref().unwrap())
    })
}

/// `i3` read whole with `FIRST-END` last, then named at a newer version
/// whose output ends `SECOND-END`: the gateway serving it.
fn newer_named(
    cx: &mut TestAppContext,
) -> (
    Entity<Irc>,
    std::sync::Arc<super::Gateway>,
    &mut VisualTestContext,
) {
    let (board, served, cx) = served(cx, 6, &format!("{}FIRST-END", "x".repeat(20_000)));
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("FIRST-END"));
    served.serve(7, tool(7, &format!("{}SECOND-END", "y".repeat(20_000))));
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(7, bytes))), cx);
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Load the latest"));
    (board, served, cx)
}

/// Round 6 thermo (P2 1), SUB decision `4311830a` item 2: while the newer
/// version is read, the older stays shown and charged beside it; it is
/// swapped only once the newer arrives.
#[gpui_kit::test]
fn load_the_latest_keeps_what_is_shown_until_the_newer_arrives(cx: &mut TestAppContext) {
    let (board, _served, cx) = newer_named(cx);
    let shown = held(&board, cx);
    cx.update(|window, cx| {
        window.click(SharedString::from(on("load")), cx);
        window.render_frame(cx);
    });
    assert!(
        drawn(&on("out:0"), cx).ends_with("FIRST-END"),
        "still shown"
    );
    assert_eq!(drawn(&on("loading"), cx), "Loading the latest…");
    assert!(held(&board, cx) > shown, "charged beside the newer read");
    settle(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("SECOND-END"), "swapped");
    assert_eq!(label(&on("load"), cx), None);
}

/// Round 6 thermo (P2 1), SUB decision `4311830a` item 2: when the newer
/// read fails, what was shown stays, beside the error and a retry.
#[gpui_kit::test]
fn a_failed_load_the_latest_keeps_what_is_shown_beside_a_retry(cx: &mut TestAppContext) {
    let (_board, served, cx) = newer_named(cx);
    *served.busy_first.lock().unwrap() = true;
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("FIRST-END"), "kept");
    assert!(drawn(&on("failed"), cx).starts_with("Stargate is busy"));
    assert_eq!(label(&on("load"), cx).as_deref(), Some("Try again"));
    *served.busy_first.lock().unwrap() = false;
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("SECOND-END"));
    assert!(drawn(&on("failed"), cx).is_empty());
}

/// Round 6 thermo (P2 1), SUB decision `4311830a` item 2: a read that
/// completes after a later request leaves that request the latest.
#[gpui_kit::test]
fn an_answer_never_becomes_the_latest_request(cx: &mut TestAppContext) {
    let (board, served, cx) = served(cx, 6, "short");
    let bytes = served.item.lock().unwrap().1.len();
    let mut items = paged(6, bytes);
    let reply = json!({"id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2,
                       "kind": "text", "status": "completed", "text": "a reply"});
    items.as_array_mut().unwrap().insert(0, reply);
    refresh(&board, cx);
    answer(&board, Ok(running(items)), cx);
    let i1 = ("t-now".to_owned(), "i1".to_owned());
    cx.update(|window, cx| window.click(SharedString::from(on("load")), cx));
    board.update(cx, |board, _| {
        ask_for(board.inspector_activity.as_mut().unwrap(), "t-now", "i1");
    });
    settle(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("short"), "read");
    let latest = board.read_with(cx, |board, _| {
        latest_request(board.inspector_activity.as_ref().unwrap())
    });
    assert_eq!(latest, Some(i1));
}
