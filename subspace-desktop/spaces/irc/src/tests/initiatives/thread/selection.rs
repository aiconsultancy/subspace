//! The message row previews threads, with an explicit reply pin.

use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::{MouseButton, SharedString, TestAppContext, point, px};
use subspace_client_irc::Event;

use super::{
    MESSAGE_TAGS, SHIP, channel, finished, front, inject, message, reply_input, seq, settle,
    thread_tab,
};

#[gpui_kit::test]
fn a_row_previews_its_thread_and_reply_from_the_gutter_pins_it(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    let room = channel(SHIP);
    let mut another = message(&room, "mira", "A separate topic", 49);
    another.id = Some("another".to_owned());
    inject(&hub, vec![Event::Line(another)], cx);
    settle(cx);
    thread_tab(&board, cx);
    let (ask, other) = (seq(&hub, "ask1", cx), seq(&hub, "another", cx));
    let text = format!("rich-menu-Name(\"irc-line:{ask}:text\")");
    let ask = SharedString::from(format!("irc-line:{ask}:row"));
    let other = SharedString::from(format!("irc-line:{other}:row"));
    let gutter = || ClickOptions::new().with_offset(point(px(13.), px(12.)));
    cx.update(|window, cx| window.click_at(text, point(px(13.), px(9.)), cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.threads.chosen(&room), Some("ask1"))
    });
    cx.update(|window, cx| window.click_with_options(other.clone(), gutter(), cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.threads.chosen(&room), Some("another"))
    });
    cx.update(|window, cx| {
        window.click_with_options(ask, gutter().with_button(MouseButton::Right), cx)
    });
    settle(cx);
    cx.update(|window, cx| {
        let mut menu = window.within("popup-menu");
        assert_eq!(menu.find(0usize).label(), Some("Reply in thread"));
        menu.click(0usize, cx);
    });
    settle(cx);
    cx.simulate_input("Pinned reply draft");
    let conversation = front(&board, cx);
    reply_input(&conversation, "ask1", cx).read_with(cx, |input, cx| {
        assert_eq!(input.text(cx), "Pinned reply draft");
    });
    cx.update(|window, cx| window.click_with_options(other.clone(), gutter(), cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.threads.chosen(&room), Some("ask1"));
        assert!(board.threads.is_pinned(&room));
    });
    cx.update(|window, cx| window.click("irc-thread-follow", cx));
    settle(cx);
    cx.update(|window, cx| window.click_with_options(other, gutter(), cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.threads.chosen(&room), Some("another"))
    });
}
