//! The conversation a Back on another Space returns to.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::Event;
use subspace_framework::dashboard::{Dashboard as _, DashboardEvent, ReturnLocation};

use super::issues::open_board;
use super::remove::settle;
use super::{channel_events, inject, mount, settings};
use crate::conversation::ConversationEvent;
use crate::dock::{self, Where};
use crate::fixtures::{channel, message, query};
use crate::markup::Ref;
use crate::{Irc, remove, view};

fn location(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Option<ReturnLocation> {
    cx.update(|_, cx| board.read(cx).return_location(cx))
}

fn return_to(board: &Entity<Irc>, key: &str, cx: &mut VisualTestContext) -> bool {
    let shown =
        cx.update(|window, cx| board.update(cx, |board, cx| board.return_to(key, window, cx)));
    settle(cx);
    shown
}

fn focused(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Option<String> {
    board.read_with(cx, |board, _| {
        board
            .dock
            .focused
            .as_ref()
            .map(|buffer| format!("{:?}", buffer.target))
    })
}

#[gpui_kit::test]
fn the_focused_channel_is_the_place_to_return_to(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    board.update(cx, |board, cx| {
        view::reveal(board, &channel("#subspace"), cx)
    });
    settle(cx);
    let place = location(&board, cx).expect("the focused channel");
    assert_eq!(place.label.as_ref(), "#subspace");

    // Another conversation comes to the front; Back returns to the channel.
    inject(
        &hub,
        vec![subspace_client_irc::Event::Line(message(
            &query("mira"),
            "mira",
            "hi",
            4,
        ))],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &query("mira"), cx));
    settle(cx);
    assert_eq!(location(&board, cx).unwrap().label.as_ref(), "mira");
    assert!(return_to(&board, &place.key, cx));
    assert_eq!(
        focused(&board, cx).as_deref(),
        Some("Channel(\"#subspace\")")
    );
}

#[gpui_kit::test]
fn a_channel_left_since_cannot_be_returned_to(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    settle(cx);
    let place = location(&board, cx).unwrap();
    board.update(cx, |board, cx| remove::remove(board, &room, cx));
    settle(cx);
    assert!(!return_to(&board, &place.key, cx));
    assert_ne!(
        focused(&board, cx).as_deref(),
        Some("Channel(\"#subspace\")")
    );
}

#[gpui_kit::test]
fn a_channel_parted_or_kicked_from_by_the_server_cannot_be_returned_to(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    let room = channel("#subspace");
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    settle(cx);
    let place = location(&board, cx).unwrap();
    // The server reports the departure: the transcript stays, unjoined.
    inject(
        &hub,
        vec![Event::Left {
            buffer: room.clone(),
        }],
        cx,
    );
    settle(cx);
    assert!(hub.read_with(cx, |hub, _| hub.state.buffer(&room).is_some()));
    assert!(!return_to(&board, &place.key, cx));

    // Joined again, it is a place to return to once more.
    inject(
        &hub,
        vec![Event::Joined {
            buffer: room.clone(),
        }],
        cx,
    );
    settle(cx);
    assert!(return_to(&board, &place.key, cx));
}

#[gpui_kit::test]
fn an_issue_opened_beside_the_focused_conversation_returns_to_its_own(cx: &mut TestAppContext) {
    let (board, hub, _tracker, cx) = open_board(cx, true, "rocket ready?");
    let (room, ops) = (channel("#subspace"), channel("#ops"));
    inject(
        &hub,
        vec![
            Event::Joined {
                buffer: ops.clone(),
            },
            Event::Line(message(&ops, "dave", "TST-13 is still loading", 4)),
        ],
        cx,
    );
    // #ops beside, then #subspace focused again.
    for (buffer, place) in [(&ops, Where::Right), (&room, Where::Tab)] {
        let buffer = buffer.clone();
        board.update(cx, |board, cx| dock::request(board, buffer, place, cx));
        settle(cx);
    }
    cx.update(|window, cx| window.render_frame(cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&room));
    });
    let origin = Rc::new(RefCell::new(None));
    let _subscription = cx.update(|_, cx| {
        let received = origin.clone();
        cx.subscribe(&board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Navigate(link) = event {
                *received.borrow_mut() = Some(link.origin.clone());
            }
        })
    });
    // The issue chip in #ops is clicked before focus moves to it.
    let beside = board.read_with(cx, |board, _| board.dock.open[&ops].entity.clone());
    beside.update(cx, |_, cx| {
        cx.emit(ConversationEvent::Open(Ref::Issue("TST-13".into())));
    });
    settle(cx);
    let place = origin
        .borrow_mut()
        .take()
        .expect("the issue was opened")
        .expect("the conversation it was opened from");
    assert_eq!(place.label.as_ref(), "#ops");
    assert!(return_to(&board, &place.key, cx));
    assert_eq!(focused(&board, cx).as_deref(), Some("Channel(\"#ops\")"));
}

#[gpui_kit::test]
fn an_unknown_place_cannot_be_returned_to(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    assert!(!return_to(&board, "not a buffer", cx));
    let missing = serde_json::to_string(&channel("#elsewhere")).unwrap();
    assert!(!return_to(&board, &missing, cx));
}
