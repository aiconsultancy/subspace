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
use crate::view::inspector::activity::{drawn_bytes, set_room};

/// The bytes of each reply's text.
const TEXT: usize = 200;

/// A reply `id` of `TEXT` bytes of `fill`.
fn reply(id: &str, fill: &str) -> Value {
    json!({"id": id, "seq": 4, "startedAt": 1, "updatedAt": 2, "kind": "text",
           "status": "completed", "text": fill.repeat(TEXT)})
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
