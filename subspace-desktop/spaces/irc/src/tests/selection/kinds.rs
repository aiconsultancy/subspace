//! Every kind of row in a drag, plain and mIRC-coded text shown as sent,
//! and links and references in them still opening.

use gpui_kit::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};
use subspace_client_irc::{BufferKey, Command, LineKind};

use super::{bounds, column, copied_rows, drag_rows, privmsgs, row_ids, shown, text_start, turns};
use crate::fixtures::{channel, line, message, network_id};
use crate::hub::Hub;

/// A notice, an action, a topic change (a compact server line) and a
/// two-line message copy too: each row is one text view, and a message's
/// line break stays inside its row.
#[gpui_kit::test]
fn notices_actions_server_lines_and_multi_line_messages_copy(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let lines = vec![
        message(&room, "alice", "first", 1),
        line(&room, LineKind::Notice, "bob", "a notice", 2),
        line(&room, LineKind::Action, "alice", "waves", 3),
        line(&room, LineKind::Topic, "bob", "Launch at noon", 4),
        message(&room, "bob", "line one\nline two", 5),
    ];
    let (hub, cx) = shown(cx, &room, lines, Vec::new());
    assert_eq!(
        drag_rows(&hub, &room, (0, 4), cx),
        copied_rows(&[
            "first",
            "a notice",
            "waves",
            "bob Launch at noon",
            "line one\nline two",
        ])
    );
}

/// Clicks the start of row `row`'s text.
fn click_row(hub: &Entity<Hub>, room: &BufferKey, row: usize, cx: &mut VisualTestContext) {
    let ids = row_ids(hub, room, cx);
    let column = column(hub, room, cx);
    let at = text_start(bounds(&ids[row], cx), column) + point(px(4.), px(0.));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

/// A URL in a plain body still opens through the link policy, and a member
/// reference in an mIRC-coded body still opens a query with them.
#[gpui_kit::test]
fn links_and_references_in_plain_and_coded_bodies_open(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let lines = turns(
        &room,
        1,
        &privmsgs(&["https://example.org/x", "\u{2}@bob\u{2} have a look"]),
    );
    let (hub, cx) = shown(cx, &room, lines, Vec::new());
    click_row(&hub, &room, 0, cx);
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.org/x"));
    click_row(&hub, &room, 1, cx);
    hub.read_with(cx, |hub, _| {
        assert!(hub.sent().contains(&Command::Query {
            network: network_id(),
            nick: "bob".into()
        }));
    });
}

/// Plain and mIRC-coded bodies show exactly as sent: Markdown characters
/// in them stay as typed, and so does a bare URL.
#[gpui_kit::test]
fn plain_and_coded_bodies_copy_exactly_as_sent(cx: &mut TestAppContext) {
    let room = channel("#subspace");
    let texts = [
        "# not a heading",
        "<tag> & a | b",
        "see https://example.org/a_b",
        "\u{3}4*not bold*\u{3}",
        "\u{2}`x` and [a](b)\u{2}",
        "\u{1f}1. not a list\u{1f}",
    ];
    let lines = turns(&room, 1, &privmsgs(&texts));
    let (hub, cx) = shown(cx, &room, lines, Vec::new());
    assert_eq!(
        drag_rows(&hub, &room, (0, 5), cx),
        copied_rows(&[
            "# not a heading",
            "<tag> & a | b",
            "see https://example.org/a_b",
            "*not bold*",
            "`x` and [a](b)",
            "1. not a list",
        ])
    );
}
