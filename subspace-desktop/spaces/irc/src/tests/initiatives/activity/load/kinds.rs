//! A paged item of a kind this version does not know, or of a known
//! kind in a changed shape, shows its JSON to the end.

use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{
    Gateway, Irc, Value, answer, drawn, json, label, load, on, paged, refresh, running, served,
};

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
