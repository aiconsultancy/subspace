//! A paged item of a kind this version does not know, or of a known
//! kind in a changed shape, shows its JSON to the end.

use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{
    Gateway, Irc, Value, answer, drawn, held_bytes, json, label, load, loaded_keys, on, paged,
    refresh, running, served, tool,
};
use crate::view::inspector::activity::{drawn_bytes, inline_bytes};

/// The view's turn view with `i3` served as `item` at `seq`.
fn served_as(
    cx: &mut TestAppContext,
    seq: u64,
    item: Value,
) -> (Entity<Irc>, Arc<Gateway>, &mut VisualTestContext) {
    let (board, served, cx) = served(cx, seq, "short");
    served.serve(seq, item);
    let bytes = served.item.lock().unwrap().1.len();
    refresh(&board, cx);
    answer(&board, Ok(running(paged(seq, bytes))), cx);
    (board, served, cx)
}

/// GPT review P1 at `9c53ac21`: a paged item of a kind this version does
/// not know loads and shows as its JSON, down to its last bytes.
#[gpui_kit::test]
fn a_paged_item_of_an_unknown_kind_shows_its_json_to_the_end(cx: &mut TestAppContext) {
    let body = format!("{}UNKNOWN-KIND-END", "u".repeat(100_000));
    let item = json!({"id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2,
                      "kind": "patch", "body": body});
    let (_board, served, cx) = served_as(cx, 6, item);
    load(cx);
    assert_eq!(served.asked().len(), 2, "both pages: {:?}", served.asked());
    assert!(!drawn(&on("unknown"), cx).is_empty());
    assert!(drawn(&on("json"), cx).contains("UNKNOWN-KIND-END"));
    assert_eq!(label(&on("load"), cx), None, "read in full");
}

/// GPT review P1 at `9c53ac21`: a paged item of a known kind in a shape
/// this version does not know loads and shows as its JSON too.
#[gpui_kit::test]
fn a_paged_item_of_a_changed_shape_shows_its_json_to_the_end(cx: &mut TestAppContext) {
    let tail = format!("{}CHANGED-SHAPE-END", "c".repeat(100_000));
    let item = json!({"id": "i3", "seq": 6, "startedAt": 1, "updatedAt": 2,
                      "kind": "tool", "status": 7, "tail": tail});
    let (_board, _served, cx) = served_as(cx, 6, item);
    load(cx);
    assert!(drawn(&on("json"), cx).contains("CHANGED-SHAPE-END"));
    assert!(drawn(&on("failed"), cx).is_empty(), "never a failure");
}

/// Round 6 thermo (P2 2), SUB decision `4311830a` item 3: an item read
/// whole that the snapshot then sends inline lets go of its read and its
/// charge, and its inline body is planned like any other.
#[gpui_kit::test]
fn an_item_sent_inline_after_its_read_is_planned_and_its_read_let_go(cx: &mut TestAppContext) {
    let (board, _served, cx) = served(cx, 6, "short");
    load(cx);
    assert!(drawn(&on("out:0"), cx).ends_with("short"), "read whole");
    refresh(&board, cx);
    answer(&board, Ok(running(json!([tool(7, "INLINE-NOW")]))), cx);
    assert!(drawn(&on("out:0"), cx).ends_with("INLINE-NOW"));
    let (keys, read, drawn) = board.read_with(cx, |board, _| {
        let selection = board.inspector_activity.as_ref().unwrap();
        (
            loaded_keys(selection),
            held_bytes(selection) - inline_bytes(selection),
            drawn_bytes(selection),
        )
    });
    assert!(keys.is_empty(), "the superseded read goes: {keys:?}");
    assert_eq!(read, 0, "and its charge");
    assert!(drawn >= "INLINE-NOW".len() as u64, "planned: {drawn}");
}
