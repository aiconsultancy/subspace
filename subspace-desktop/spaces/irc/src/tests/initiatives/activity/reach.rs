//! Everything an officer's tools sent stays in reach (SUB decision
//! `33ccecf6` items 2, 4 and 5): a card's heading is fixed words, while
//! names, keys and values are its measured body, drawn in parts like any
//! other; fields this version does not know show beside those it does;
//! and a body drawn in parts writes out only the part shown.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use serde_json::{Map, Value, json};

use super::super::super::remove::settle;
use super::{answer, drawn, opened, running};
use crate::Irc;
use crate::view::inspector::activity::{set_room, walked};

/// The most bytes one part of a body drawn in parts draws.
const PART: usize = 64 * 1024;
/// The card of the item under test.
const CARD: &str = "irc-activity:t-now:i2";

/// The ID of the card's element `part`.
fn on(part: &str) -> String {
    format!("{CARD}:{part}")
}

fn roomed(board: &Entity<Irc>, room: u64, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        set_room(board.inspector_activity.as_mut().unwrap(), room);
        cx.notify();
    });
    settle(cx);
}

fn click(part: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(SharedString::from(on(part)), cx));
    settle(cx);
}

/// A completed tool call `i2` named `name`, whose one argument is `key`,
/// with `fields` added to it.
fn tool(name: &str, key: &str, fields: Value) -> Value {
    let mut input = Map::new();
    input.insert(key.to_owned(), json!("v"));
    let mut tool = json!({"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2,
                          "kind": "tool", "callId": "call-1", "name": name,
                          "status": "completed", "rawInput": "", "input": input,
                          "executed": true, "metadata": null, "content": [],
                          "error": null});
    if let (Some(tool), Value::Object(fields)) = (tool.as_object_mut(), fields) {
        tool.extend(fields);
    }
    tool
}

/// Round 5 thermo (P1), SUB decision `33ccecf6` item 2: a card's heading
/// is its kind and short status alone. A tool's name and an argument's
/// key, of any length, are its body, measured with each marker at twelve
/// bytes: held back, the card mounts its heading, never them; asked for,
/// they draw in parts within the room, then whole once it fits.
#[gpui_kit::test]
fn a_held_back_card_mounts_only_its_bounded_heading(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let name = "N".repeat(200);
    let key = "\u{202E}".repeat(24);
    answer(
        &board,
        Ok(running(json!([tool(&name, &key, json!({}))]))),
        cx,
    );
    // The name, the key's 24 markers and the value.
    let bytes = 200 + 24 * 12 + 1;
    roomed(&board, bytes - 1, cx);
    assert_eq!(drawn(&on("heading"), cx), "Tool · completed");
    assert_eq!(
        drawn(&on("held-back"), cx),
        format!("Not drawn, to stay within the view's memory budget ({bytes} bytes).")
    );
    assert!(drawn(&on("name"), cx).is_empty(), "not mounted");
    assert!(drawn(&on("input:0:key"), cx).is_empty(), "not mounted");

    click("show", cx);
    assert_eq!(drawn(&on("heading"), cx), "Tool · completed");
    assert!(drawn(&on("body"), cx).starts_with("Name\nNNNN"));
    assert!(drawn(&on("piece"), cx).starts_with("Part 1 of "));
    assert!(drawn(&on("name"), cx).is_empty(), "in parts, not whole");

    roomed(&board, bytes, cx);
    assert_eq!(drawn(&on("name"), cx), name);
    assert_eq!(drawn(&on("input:0:key"), cx), "⟨U+202E⟩".repeat(24));
    assert_eq!(drawn(&on("input:0"), cx), "v");
}

/// Round 5 thermo (P2), SUB decision `33ccecf6` item 4: a known output
/// shape keeps its fields this version does not know, and so does the
/// item: each shows as its key and its value.
#[gpui_kit::test]
fn the_fields_this_version_does_not_know_are_drawn(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let fields = json!({
        "content": [{"type": "text", "text": "out", "annotations": {"beam": "BEAM-EXTRA"}}],
        "sandbox": "SANDBOX-EXTRA"
    });
    answer(
        &board,
        Ok(running(json!([tool("read", "path", fields)]))),
        cx,
    );
    assert_eq!(drawn(&on("out:0"), cx), "out");
    assert_eq!(drawn(&on("out:0:extra:0:key"), cx), "annotations");
    assert!(drawn(&on("out:0:extra:0"), cx).contains("\"beam\": \"BEAM-EXTRA\""));
    assert_eq!(drawn(&on("extra:0:key"), cx), "sandbox");
    assert_eq!(drawn(&on("extra:0"), cx), "SANDBOX-EXTRA");
}

/// Round 5 thermo (P2), SUB decision `33ccecf6` items 4 and 5: a step
/// drawn in parts shows its settled use as it does whole (SUB-70), its
/// error is drawn through the same parts, every byte in exactly one, and
/// moving to a part writes out that part alone.
#[gpui_kit::test]
fn a_step_in_parts_shows_its_use_and_writes_out_only_the_part_shown(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let response = format!("{}END", "E".repeat(200_000));
    let step = json!({"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "step",
        "status": "failed", "model": {"provider": "p", "id": "m"},
        "tokens": {"input": 10, "output": 5, "reasoning": 0, "cacheRead": 0, "cacheWrite": 0},
        "cost": 0.25,
        "error": {"type": "api", "message": "boom", "status": 500, "responseBody": response}});
    answer(&board, Ok(running(json!([step]))), cx);
    roomed(&board, 2 * PART as u64, cx);
    click("show", cx);
    assert_eq!(
        drawn(&on("use"), cx),
        "10 in · 5 out · 0 reasoning · 0 cache read · 0 cache write · $0.2500"
    );
    assert_eq!(drawn(&on("heading"), cx), "Step · failed");
    let body = format!("Provider\np\n\nModel\nm\n\nError\napi: boom\nStatus 500\n{response}");
    let parts = body.len().div_ceil(PART);
    assert_eq!(drawn(&on("piece"), cx), format!("Part 1 of {parts}"));

    let before = walked();
    click("piece-last", cx);
    assert!(
        walked() - before <= PART as u64,
        "only the last part is written out"
    );
    assert_eq!(drawn(&on("piece"), cx), format!("Part {parts} of {parts}"));
    assert!(drawn(&on("body"), cx).ends_with("END"));

    click("piece-first", cx);
    let mut joined = drawn(&on("body"), cx);
    for _ in 1..parts {
        click("piece-next", cx);
        joined.push_str(&drawn(&on("body"), cx));
    }
    assert_eq!(joined, body, "every byte in exactly one part");
}

/// Round 6 thermo (P2 3), SUB decisions `469f1320` and `4311830a` item 4:
/// the token counts a step settled that this version does not know, nested
/// ones too, show as blocks beside its settled use.
#[gpui_kit::test]
fn a_step_shows_the_token_counts_this_version_does_not_know(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let step = json!({"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "step",
        "status": "completed",
        "tokens": {"input": 10, "output": 5, "reasoning": 0, "cacheRead": 0, "cacheWrite": 0,
                   "audio": 3, "detail": {"image": "IMAGE-TOKENS"}},
        "cost": 0.25});
    answer(&board, Ok(running(json!([step]))), cx);
    assert_eq!(
        drawn(&on("use"), cx),
        "10 in · 5 out · 0 reasoning · 0 cache read · 0 cache write · $0.2500"
    );
    assert_eq!(drawn(&on("tokens:0:key"), cx), "audio");
    assert_eq!(drawn(&on("tokens:0"), cx), "3");
    assert_eq!(drawn(&on("tokens:1:key"), cx), "detail");
    assert!(drawn(&on("tokens:1"), cx).contains("\"image\": \"IMAGE-TOKENS\""));
}
