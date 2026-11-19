//! Find follows the focused dock pane, including a temporary attachment.
use super::{assert_count, attach_find, close_attachment, open_attachment};
use crate::{
    Irc,
    conversation::Conversation,
    fixtures::{channel, line},
    tests::{inject, mount, settings},
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{Event, LineKind};
fn split(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Conversation>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    let first = channel("#first");
    let second = channel("#second");
    let events = vec![
        Event::Joined {
            buffer: first.clone(),
        },
        Event::Joined {
            buffer: second.clone(),
        },
        Event::Line(line(
            &first,
            LineKind::Privmsg,
            "fixture-bot",
            "needle first",
            1,
        )),
        Event::Line(line(
            &second,
            LineKind::Privmsg,
            "fixture-bot",
            "needle second",
            2,
        )),
    ];
    inject(&hub, events, cx);
    board.update(cx, |board, cx| {
        crate::dock::request(board, first.clone(), crate::dock::Where::Tab, cx)
    });
    cx.run_until_parked();
    board.update(cx, |board, cx| {
        crate::dock::request(board, second.clone(), crate::dock::Where::Right, cx)
    });
    cx.run_until_parked();
    attach_find(&board, cx);
    cx.update(|window, cx| {
        window.press(subspace_framework::dashboard::secondary!("f"), cx);
        window.input("needle", cx);
    });
    cx.run_until_parked();
    let conversation = board.read_with(cx, |b, _| b.dock.open.get(&second).unwrap().entity.clone());
    (board, conversation, cx)
}
#[gpui_kit::test]
fn a_split_background_pane_is_excluded_while_the_focused_attachment_is_open(
    cx: &mut TestAppContext,
) {
    let (board, conversation, cx) = split(cx);
    assert_eq!(
        board.read_with(cx, |b, _| b.dock.focused.clone()),
        Some(channel("#second"))
    );
    assert_count("Match 1 of 1", cx);
    open_attachment(&conversation, cx);
    assert_count("Match 0 of 0", cx);
    close_attachment(cx);
    assert_count("Match 1 of 1", cx);
}
