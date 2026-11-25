//! How activity text shows: a tool's input as parsed and as streamed,
//! untrusted markers as code points (the separators and byte order mark
//! too, copied as shown), unknown shapes as JSON, and menus without Add
//! to chat.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{SharedString, TestAppContext};
use serde_json::json;
use subspace_framework::dashboard::Dashboard as _;

use super::super::super::remove::settle;
use super::{answer, drawn, opened, refresh, running, snapshot, turn};

#[gpui_kit::test]
fn a_tool_shows_its_input_as_parsed_and_as_streamed(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    answer(
        &board,
        Ok(running(json!([
            {"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "tool",
             "callId": "call-1", "name": "bash", "status": "running",
             "rawInput": "{\"command\": \"ls\", \"why\": \"STREAMED-ONLY\"",
             "input": {"command": "ls"}, "executed": false,
             "metadata": null, "content": null, "error": null}
        ]))),
        cx,
    );
    assert_eq!(drawn("irc-activity:t-now:i2:input:command", cx), "ls");
    assert!(
        drawn("irc-activity:t-now:i2:raw", cx).contains("STREAMED-ONLY"),
        "what only the stream carried shows too"
    );
}

#[gpui_kit::test]
fn controls_and_bidi_marks_show_as_code_points_and_unknown_shapes_as_json(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    answer(
        &board,
        Ok(running(json!([
            {"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "tool",
             "callId": "call-1", "name": "bash", "status": "completed",
             "rawInput": "", "input": null, "executed": true, "metadata": null,
             "content": [{"type": "text", "text": "\u{1b}[31mred abc\u{202E}fed"},
                         {"type": "hologram", "beam": "BEAM-SENTINEL"}],
             "error": null},
            {"id": "i5", "seq": 2, "startedAt": 1, "updatedAt": 2, "kind": "hologram",
             "depth": "DEPTH-SENTINEL"}
        ]))),
        cx,
    );
    assert_eq!(
        drawn("irc-activity:t-now:i2:out:0", cx),
        "⟨U+001B⟩[31mred abc⟨U+202E⟩fed"
    );
    assert!(drawn("irc-activity:t-now:i2:out:1", cx).contains("\"beam\": \"BEAM-SENTINEL\""));
    let unknown = drawn("irc-activity:t-now:i5:json", cx);
    assert!(unknown.contains("\"kind\": \"hologram\""), "{unknown}");
    assert!(unknown.contains("DEPTH-SENTINEL"), "{unknown}");
}

#[gpui_kit::test]
fn separators_and_the_byte_order_mark_show_as_code_points_in_replies_and_tool_input(
    cx: &mut TestAppContext,
) {
    let (board, cx) = opened(cx);
    answer(
        &board,
        Ok(running(json!([
            {"id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2, "kind": "text",
             "status": "completed", "text": "line\u{2028}para\u{2029}\u{FEFF}end"},
            {"id": "i2", "seq": 5, "startedAt": 1, "updatedAt": 2, "kind": "tool",
             "callId": "call-1", "name": "bash", "status": "running",
             "rawInput": "{\"command\": \"a\u{2028}b\u{2029}c\u{FEFF}\"",
             "input": {"command": "a\u{2028}b\u{2029}c\u{FEFF}"}, "executed": false,
             "metadata": null, "content": null, "error": null}
        ]))),
        cx,
    );
    let shown = "line⟨U+2028⟩para⟨U+2029⟩⟨U+FEFF⟩end";
    assert_eq!(drawn("irc-activity:t-now:i1:text", cx), shown);
    assert_eq!(
        drawn("irc-activity:t-now:i2:input:command", cx),
        "a⟨U+2028⟩b⟨U+2029⟩c⟨U+FEFF⟩"
    );
    let raw = drawn("irc-activity:t-now:i2:raw", cx);
    assert!(raw.contains("a⟨U+2028⟩b⟨U+2029⟩c⟨U+FEFF⟩"), "{raw}");
    // Select all, then Copy, take the text as shown, as the console does.
    for entry in [1usize, 0] {
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.right_click("irc-activity:t-now:i1:text", cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.within("popup-menu").click(entry, cx);
        });
        cx.run_until_parked();
    }
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some(shown));
}

#[gpui_kit::test]
fn activity_text_offers_copy_and_select_all_and_never_reaches_the_chat(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    answer(&board, Ok(snapshot(true, "OUTPUT-SENTINEL")), cx);
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click("irc-activity:t-now:i2:out:0", cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let menu = window.within("popup-menu");
        assert_eq!(menu.find(0usize).label(), Some("Copy"));
        assert_eq!(menu.find(1usize).label(), Some("Select all"));
        assert!(menu.try_find(2usize).is_none(), "no Add to chat");
    });
    let context = cx.update(|_, cx| board.read(cx).agent_context(cx));
    let content = context.map(|context| context.content).unwrap_or_default();
    assert!(!content.contains("OUTPUT-SENTINEL"), "{content}");
    assert!(!content.contains("Looking at"), "{content}");
}

/// Captain's ruling on round 4, point 6: a turn whose ID holds `:` keeps
/// the part shown of its long text when the snapshot is read again.
#[gpui_kit::test]
fn a_turn_id_with_a_colon_keeps_the_part_shown(cx: &mut TestAppContext) {
    let (board, cx) = opened(cx);
    let long = "w".repeat(300 * 1024);
    let colon = || {
        let items = json!([{"id": "i1", "seq": 4, "startedAt": 1, "updatedAt": 2,
                            "kind": "text", "status": "completed", "text": long}]);
        serde_json::from_value(json!({
            "agent": {"id": "aurora-lyra", "role": "captain", "backend": "opencode",
                      "collected": true},
            "turns": [turn("t:1", "running", 0, items)],
            "cursor": {"generation": "g1", "seq": 5}
        }))
        .unwrap()
    };
    answer(&board, Ok(colon()), cx);
    let id = "irc-activity:t:1:i1:text";
    cx.update(|window, cx| window.click(SharedString::from(format!("{id}:part-next")), cx));
    settle(cx);
    assert_eq!(drawn(&format!("{id}:part"), cx), "Part 2 of 5");
    refresh(&board, cx);
    answer(&board, Ok(colon()), cx);
    assert_eq!(drawn(&format!("{id}:part"), cx), "Part 2 of 5");
}
