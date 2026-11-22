//! The status line's clock redraws only while a line runs.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::Event;

use super::super::{channel_events, inject, mount, settings};
use super::{SHIP, joined, progress, request, status};
use crate::fixtures::channel;
use crate::{Irc, view};

#[gpui_kit::test]
fn the_clock_ticks_only_while_a_line_runs(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(joined("astra", SHIP));
    events.extend(
        [
            request(&room, "first", None, 4),
            progress(&room, "first", "tool", 5),
        ]
        .map(Event::Line),
    );
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    let ticking = |board: &Entity<Irc>, cx: &mut VisualTestContext| {
        board.read_with(cx, |board, cx| {
            board
                .dock
                .open
                .get(&room)
                .is_some_and(|hosted| crate::conversation::ticking(hosted.entity.read(cx)))
        })
    };
    assert!(ticking(&board, cx), "a running line ticks");
    inject(
        &hub,
        vec![Event::Line(progress(&room, "first", "failed", 9))],
        cx,
    );
    cx.run_until_parked();
    assert!(!ticking(&board, cx), "nothing ticks once it settled");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            status(window, "first").as_deref(),
            Some("astra: Failed after 5 m")
        );
    });
}
