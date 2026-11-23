//! The reaction menu opens at the message it is for: the toolbar's React at
//! the hovered message, `r` at the keyboard-focused one.

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;

use super::{open, room, seq};

#[gpui_kit::test]
fn the_toolbar_s_react_opens_at_the_hovered_message_not_the_focused_one(cx: &mut TestAppContext) {
    let (board, hub, cx) = open(cx);
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room()].entity.clone());
    let m1 = seq(&hub, "m1", cx);
    conversation.update(cx, |view, cx| {
        crate::conversation::hover(view, m1, true, cx)
    });
    cx.update(|window, cx| {
        conversation.update(cx, |view, cx| {
            crate::conversation::reacting::open_choices_for(view, m1, window, cx);
        });
        window.render_frame(cx);
    });
    conversation.read_with(cx, |view, cx| {
        assert!(
            view.reacting.hover_anchor.read(cx).is_open(),
            "at the hovered message"
        );
        assert!(
            !view.reacting.anchor.read(cx).is_open(),
            "not at the focused one's anchor"
        );
    });
}
