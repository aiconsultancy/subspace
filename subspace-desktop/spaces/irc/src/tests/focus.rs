//! Keyboard focus in the transcript: into it from the message input, between
//! messages and references, opening, and back to the input.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc};
use subspace_framework::dashboard::DashboardEvent;

use super::completion::text;
use super::issues::open_board;
use crate::conversation::Conversation;
use crate::fixtures::channel;
use crate::transcript::focus::TranscriptFocus;

fn focused(
    conversation: &Entity<Conversation>,
    cx: &mut VisualTestContext,
) -> Option<TranscriptFocus> {
    cx.update(|window, cx| window.render_frame(cx));
    conversation.read_with(cx, |conversation, _| {
        crate::conversation::focused(conversation)
    })
}

fn at(seq: i64, reference: Option<usize>) -> Option<TranscriptFocus> {
    Some(TranscriptFocus { seq, reference })
}

#[gpui_kit::test]
fn the_keyboard_walks_messages_and_references(cx: &mut TestAppContext) {
    let (board, _, _, cx) = open_board(cx, true, "TST-12 blocks #release, see https://example.org");
    cx.update(|_, cx| {
        subspace_framework::dashboard::chat_input::bind_keys(cx);
        crate::conversation::bind_keys(cx);
    });
    let room = channel("#subspace");
    let conversation = board.read_with(cx, |board, _| board.dock.open[&room].entity.clone());
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    cx.update(|window, cx| input.update(cx, |input, cx| input.focus(window, cx)));

    cx.simulate_keystrokes("shift-tab");
    assert_eq!(
        focused(&conversation, cx),
        at(2, None),
        "the newest message"
    );
    cx.simulate_keystrokes("tab");
    assert_eq!(focused(&conversation, cx), at(2, Some(0)), "TST-12");
    assert!(
        conversation.read_with(cx, |conversation, _| crate::conversation::has_card(
            conversation
        )),
        "a focused issue shows its card"
    );
    assert_issue_opens(&board, cx);
    cx.simulate_keystrokes("tab tab tab");
    assert_eq!(
        focused(&conversation, cx),
        at(2, Some(2)),
        "stays on the link"
    );
    assert!(
        !conversation.read_with(cx, |conversation, _| crate::conversation::has_card(
            conversation
        ))
    );
    cx.simulate_keystrokes("up");
    assert_eq!(
        focused(&conversation, cx),
        at(1, None),
        "the previous message"
    );
    cx.simulate_keystrokes("down down");
    assert_eq!(
        focused(&conversation, cx),
        at(2, None),
        "stops at the newest"
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(focused(&conversation, cx), None);
    cx.simulate_input("x");
    assert_eq!(text(&input, cx), "x", "back in the input");
}

fn assert_issue_opens(board: &Entity<crate::Irc>, cx: &mut VisualTestContext) {
    let destination = Rc::new(RefCell::new(None));
    let _subscription = cx.update(|_, cx| {
        let destination = destination.clone();
        cx.subscribe(board, move |_, event: &DashboardEvent, _| {
            if let DashboardEvent::Navigate(link) = event {
                *destination.borrow_mut() = Some(link.url.clone());
            }
        })
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(
        destination.borrow().as_deref(),
        Some("https://api.example.test/api/v1/issues/TST-12?profile=dev"),
        "Enter opens the issue's page"
    );
}
