//! What the open turns draw counts against the view's budget (SUB
//! decision `ed71a299`): a body that would pass the room left shows its
//! label, its size and **Show**; **Show** draws it within the budget, and
//! a collapsed turn draws nothing.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use subspace_client_fleet::activity::AgentActivity;

use super::super::super::remove::settle;
use super::{answer, drawn, opened, turn};
use crate::Irc;
use crate::view::inspector::activity::{drawn_bytes, measured, set_room};

/// The bytes of each reply's text.
const TEXT: usize = 200;
/// The most bytes one part of a body drawn in parts draws.
const PART: usize = 64 * 1024;

/// A reply `id` of `TEXT` bytes of `fill`.
fn reply(id: &str, fill: &str) -> Value {
    json!({"id": id, "seq": 4, "startedAt": 1, "updatedAt": 2, "kind": "text",
           "status": "completed", "text": fill.repeat(TEXT)})
}

/// A reply `id` whose text is `text`.
fn reply_of(id: &str, text: &str) -> Value {
    json!({"id": id, "seq": 4, "startedAt": 1, "updatedAt": 2, "kind": "text",
           "status": "completed", "text": text})
}

/// A completed turn with one reply, and the running turn with `items`.
fn activity(items: Value) -> AgentActivity {
    serde_json::from_value(json!({
        "agent": {"id": "aurora-lyra", "role": "captain", "backend": "opencode",
                  "collected": true},
        "turns": [turn("t-old", "completed", 0, json!([reply("o1", "o")])),
                  turn("t-now", "running", 0, items)],
        "cursor": {"generation": "g1", "seq": 5}
    }))
    .unwrap()
}

/// `room` bytes left beside the inline content.
fn roomed(board: &Entity<Irc>, room: u64, cx: &mut VisualTestContext) {
    board.update(cx, |board, cx| {
        set_room(board.inspector_activity.as_mut().unwrap(), room);
        cx.notify();
    });
    settle(cx);
}

fn drawn_total(board: &Entity<Irc>, cx: &mut VisualTestContext) -> u64 {
    board.read_with(cx, |board, _| {
        drawn_bytes(board.inspector_activity.as_ref().unwrap())
    })
}

fn click(id: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(SharedString::from(id.to_owned()), cx));
    settle(cx);
}

/// Whether item `item` of the running turn draws its body.
fn shows(item: &str, fill: &str, cx: &mut VisualTestContext) -> bool {
    let body = drawn(&format!("irc-activity:t-now:{item}:text"), cx).contains(&fill.repeat(64));
    let held = drawn(&format!("irc-activity:t-now:{item}:held-back"), cx);
    assert_eq!(body, held.is_empty(), "{item}: body or caption, not both");
    body
}

/// Captain's ruling on round 4, point 1: an open turn draws its bodies
/// only up to the room left, the rest show their size and **Show**, and
/// **Show** draws the one asked for within the budget.
#[gpui_kit::test]
fn an_open_turn_draws_within_the_room_and_show_draws_the_rest(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let items = json!([reply("i1", "a"), reply("i2", "b"), reply("i3", "c")]);
    answer(&board, Ok(activity(items)), cx);
    let room = (2 * TEXT + TEXT / 2) as u64;
    roomed(&board, room, cx);
    assert!(shows("i1", "a", cx));
    assert!(shows("i2", "b", cx));
    assert!(!shows("i3", "c", cx));
    assert_eq!(
        drawn("irc-activity:t-now:i3:held-back", cx),
        format!("Not drawn, to stay within the view's memory budget ({TEXT} bytes).")
    );
    assert_eq!(drawn_total(&board, cx), 2 * TEXT as u64);

    click("irc-activity:t-now:i3:show", cx);
    assert!(shows("i3", "c", cx), "drawn once asked for");
    assert!(shows("i1", "a", cx));
    assert!(!shows("i2", "b", cx), "the next in order no longer fits");
    assert!(drawn_total(&board, cx) <= room);
}

/// Captain's ruling on round 4, point 1: a collapsed turn builds no
/// bodies, so it draws nothing; once opened it draws first.
#[gpui_kit::test]
fn a_collapsed_turn_draws_nothing(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    answer(&board, Ok(activity(json!([reply("i1", "a")]))), cx);
    roomed(&board, (TEXT + TEXT / 2) as u64, cx);
    assert_eq!(
        drawn_total(&board, cx),
        TEXT as u64,
        "the old turn is closed"
    );
    assert!(shows("i1", "a", cx));

    click("irc-activity-turn:t-old", cx);
    assert!(drawn("irc-activity:t-old:o1:text", cx).contains(&"o".repeat(64)));
    assert!(!shows("i1", "a", cx), "the turn opened last draws first");
    assert_eq!(drawn_total(&board, cx), TEXT as u64);
}

/// Round 4 review (P1), SUB decision `0c13b784` §1: a body counts the
/// bytes it draws, each control character's `⟨U+XXXX⟩` marker in full,
/// twelve bytes where the text has one.
#[gpui_kit::test]
fn a_body_of_control_characters_counts_each_marker_in_full(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let items = json!([reply_of("i1", &"\u{0}".repeat(TEXT))]);
    answer(&board, Ok(activity(items)), cx);
    let marked = 12 * TEXT as u64;
    roomed(&board, marked - 1, cx);
    assert_eq!(
        drawn("irc-activity:t-now:i1:held-back", cx),
        format!("Not drawn, to stay within the view's memory budget ({marked} bytes).")
    );
    assert_eq!(drawn_total(&board, cx), 0);

    roomed(&board, marked, cx);
    assert!(drawn("irc-activity:t-now:i1:text", cx).starts_with("⟨U+0000⟩⟨U+0000⟩"));
    assert_eq!(drawn_total(&board, cx), marked);
}

/// Round 4 reviews (P2), SUB decision `0c13b784` §2: each body is measured
/// and its values formatted once per version of its item, never again for
/// a frame; the formatted text kept counts as held.
#[gpui_kit::test]
fn drawing_again_measures_and_formats_nothing(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let unknown = json!({"id": "u1", "seq": 4, "startedAt": 1, "updatedAt": 2,
                         "kind": "patch", "body": {"k": "v".repeat(TEXT)}});
    let pretty = serde_json::to_string_pretty(&unknown).unwrap().len() as u64;
    answer(&board, Ok(activity(json!([reply("i1", "a"), unknown]))), cx);
    assert!(drawn("irc-activity:t-now:u1:json", cx).contains(&"v".repeat(TEXT)));
    // The reply as drawn; the item's JSON as drawn and as kept formatted.
    assert_eq!(drawn_total(&board, cx), TEXT as u64 + 2 * pretty);
    let before = measured();
    assert!(before > 0);
    for _ in 0..3 {
        board.update(cx, |_, cx| cx.notify());
        settle(cx);
    }
    assert!(drawn("irc-activity:t-now:u1:json", cx).contains(&"v".repeat(TEXT)));
    assert_eq!(measured(), before, "no frame measures or formats again");

    click("irc-activity-turn:t-old", cx);
    assert!(measured() > before, "a turn opened is measured");
}

/// Round 4 reviews (P3), SUB decision `0c13b784` §3: a body larger than
/// the room even once room was made is drawn one part of at most 64 KiB
/// at a time, every part in reach: never hidden and never cut.
#[gpui_kit::test]
fn a_body_past_the_room_is_drawn_in_parts_once_asked_for(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let text = format!("{}END", "x".repeat(2 * PART));
    answer(&board, Ok(activity(json!([reply_of("big", &text)]))), cx);
    roomed(&board, PART as u64, cx);
    assert!(drawn("irc-activity:t-now:big:held-back", cx).starts_with("Not drawn"));

    click("irc-activity:t-now:big:show", cx);
    assert!(
        drawn("irc-activity:t-now:big:pieced", cx)
            .starts_with("Too large to draw whole within the view's memory budget")
    );
    assert!(
        drawn("irc-activity:t-now:big:text", cx).is_empty(),
        "not whole"
    );
    assert_eq!(drawn("irc-activity:t-now:big:piece", cx), "Part 1 of 3");
    assert_eq!(drawn("irc-activity:t-now:big:body", cx), "x".repeat(PART));
    // One part and its drawn copy: the floor.
    assert_eq!(drawn_total(&board, cx), 2 * PART as u64);

    let mut joined = drawn("irc-activity:t-now:big:body", cx);
    for part in 2..=3 {
        click("irc-activity:t-now:big:piece-next", cx);
        assert_eq!(
            drawn("irc-activity:t-now:big:piece", cx),
            format!("Part {part} of 3")
        );
        joined.push_str(&drawn("irc-activity:t-now:big:body", cx));
    }
    assert_eq!(joined, text, "every byte in exactly one part");
    click("irc-activity:t-now:big:piece-first", cx);
    assert_eq!(drawn("irc-activity:t-now:big:piece", cx), "Part 1 of 3");
}
