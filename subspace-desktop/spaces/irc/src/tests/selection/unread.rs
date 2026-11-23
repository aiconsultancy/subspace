//! The unread divider remains a label outside the selectable body sequence.

use gpui_kit::TestAppContext;
use gpui_kit::base::TextSelection;
use gpui_kit::base::input::Copy;
use gpui_kit::test::TestWindowExt as _;

use super::{copied_rows, drag_rows, shown};
use crate::fixtures::{channel, message};

#[gpui_kit::test]
fn a_drag_across_the_unread_divider_copies_both_sides_through_the_copy_action(
    cx: &mut TestAppContext,
) {
    let room = channel("#subspace");
    let older = vec![message(&room, "alice", "*Read* context", 1)];
    let live = vec![
        message(&room, "bob", "Unread plain update for @alice", 2),
        message(&room, "alice", "\u{3}4Unread FAIL\u{3}", 3),
    ];
    let (hub, cx) = shown(cx, &room, live, older);
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-unread:0").label(),
            Some("2 new messages since you left")
        );
        let drawn: Vec<String> = gpui_kit::base::test_support::texts(window)
            .into_iter()
            .filter(|text| text.path().contains(&"irc-unread:0".into()) && text.visible())
            .map(|text| text.text().to_owned())
            .collect();
        assert_eq!(drawn, ["NEW SINCE YOU LEFT · 2"]);
    });
    hub.read_with(cx, |hub, _| {
        let held = hub.state.buffer(&room).unwrap();
        assert_eq!(held.unread, 0, "opening cleared the badge");
        assert_eq!(held.shown_unread().len(), 2, "the tint stays while reading");
    });
    let expected = copied_rows(&[
        "Read context",
        "Unread plain update for @alice",
        "Unread FAIL",
    ]);
    assert_eq!(drag_rows(&hub, &room, (0, 2), cx), expected);
    cx.update(|window, cx| {
        let focus = window.focused(cx).expect("drag focuses the text view");
        focus.dispatch_action(&Copy, window, cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(expected.trim().to_string())
        );
        assert_eq!(TextSelection::selected_text(window, cx), expected);
        assert!(focus.is_focused(window));
    });
}
