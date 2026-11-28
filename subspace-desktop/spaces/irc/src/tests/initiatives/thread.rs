//! The inspector's Thread tab on the board: the prompt before a thread is
//! chosen, a request's status line carrying its reply count and opening the
//! thread, the root, timeline and replies as drawn, new replies arriving
//! live, the chosen thread kept per conversation, and reply mode in the
//! composer: what it sends, to where, and its refusal without the reply tag.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{Event, Line, LineKind};

use super::super::inject;
use super::super::remove::settle;
use super::fleet::{ship, signed};
use crate::conversation::Conversation;
use crate::fixtures::{channel, line, message, network_id};
use crate::hub::Hub;
use crate::thread::{MESSAGE_TAGS, NO_REPLY_TAG};
use crate::thread_status::PROGRESS_TAG;
use crate::view::InspectorTab;
use crate::{Irc, view};

mod composing;
mod live;
pub(super) use composing::reply_input;

const SHIP: &str = "#aurora-lyra";

/// mira's request `ask1` to the captain at minute 4.
fn asked() -> Line {
    let mut line = message(&channel(SHIP), "mira", "aurora-lyra: capture a baseline", 4);
    line.id = Some("ask1".into());
    line
}

/// The captain's progress notice on `ask1`, signed in to its own account.
fn progress(value: &str, minute: i64) -> Line {
    let mut line = signed(line(
        &channel(SHIP),
        LineKind::Notice,
        "aurora-lyra",
        "Prose never read.",
        minute,
    ));
    line.id = Some(format!("p-{value}-{minute}"));
    line.reply_to = Some("ask1".into());
    line.client_tags.insert(PROGRESS_TAG.into(), value.into());
    line
}

/// `nick`'s reply `id` to `parent`.
fn reply(nick: &str, id: &str, parent: &str, minute: i64) -> Line {
    let text = format!("Reply {id}.");
    let mut line = signed(message(&channel(SHIP), nick, &text, minute));
    line.id = Some(id.into());
    line.reply_to = Some(parent.into());
    line
}

/// The ship's channel with mira's request, its finished timeline and two
/// replies, on a connection that agreed `caps`.
pub(super) fn finished<'a>(
    cx: &'a mut TestAppContext,
    caps: &[&str],
) -> (Entity<Irc>, Entity<Hub>, &'a mut VisualTestContext) {
    let (board, hub, cx) = ship(cx);
    inject(
        &hub,
        vec![
            Event::Capabilities {
                network: network_id(),
                enabled: caps.iter().map(|cap| (*cap).to_owned()).collect(),
            },
            Event::Line(asked()),
            Event::Line(progress("accepted", 5)),
            Event::Line(progress("tool", 7)),
            Event::Line(progress("completed", 46)),
            Event::Line(reply("aurora-lyra", "r1", "ask1", 46)),
            Event::Line(reply("mira", "r2", "r1", 48)),
        ],
        cx,
    );
    settle(cx);
    (board, hub, cx)
}

pub(super) fn front(board: &Entity<Irc>, cx: &mut VisualTestContext) -> Entity<Conversation> {
    board.read_with(cx, |board, _| {
        board.dock.open[&channel(SHIP)].entity.clone()
    })
}

/// Shows the inspector's Thread tab, as its tab's click does.
fn thread_tab(board: &Entity<Irc>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.click(("workspace-inspector-tab", 1usize), cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.inspector_tab, InspectorTab::Thread);
    });
}

#[gpui_kit::test]
fn with_no_thread_chosen_the_tab_says_how_to_open_one(cx: &mut TestAppContext) {
    let (board, _, cx) = ship(cx);
    thread_tab(&board, cx);
    cx.update(|window, _| {
        assert_eq!(
            window.find("irc-thread-prompt-line").label(),
            Some(view::THREAD_PROMPT)
        );
        assert!(window.try_find("irc-reply-editor").is_none(), "no footer");
    });
}

#[gpui_kit::test]
fn the_settled_line_carries_the_reply_count_and_opens_the_thread(cx: &mut TestAppContext) {
    let (board, _, cx) = finished(cx, &[MESSAGE_TAGS]);
    cx.update(|window, _| {
        let line = window.find("irc-status:ask1");
        assert!(
            line.label()
                .is_some_and(|label| label.contains("2 replies")),
            "{:?}",
            line.label()
        );
        assert!(
            window.try_find("irc-replies:ask1").is_none(),
            "the count moves onto the status line"
        );
    });
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.inspector_tab, InspectorTab::Thread);
        assert_eq!(board.threads.chosen(&channel(SHIP)), Some("ask1"));
    });
    cx.update(|window, _| {
        assert!(window.find("irc-thread").visible());
        let steps: Vec<_> = (0..3)
            .map(|index| {
                window
                    .find(format!("irc-thread-step:{index}"))
                    .label()
                    .map(str::to_owned)
                    .unwrap_or_default()
            })
            .collect();
        assert!(steps[0].starts_with("Accepted"), "{steps:?}");
        assert!(steps[1].starts_with("Working"), "{steps:?}");
        assert!(
            steps[2].starts_with("Done") && steps[2].ends_with("in 42 m"),
            "{steps:?}"
        );
        let replies = window.within("irc-thread-replies");
        assert!(
            replies
                .try_find_by_label("aurora-lyra: Reply r1.")
                .is_some()
        );
        assert!(
            replies.try_find_by_label("mira: Reply r2.").is_some(),
            "a reply to a reply"
        );
        assert!(window.find("irc-reply-editor").visible());
    });
}

#[gpui_kit::test]
fn a_reply_count_opens_a_message_s_thread_and_new_replies_arrive_live(cx: &mut TestAppContext) {
    let (_, hub, cx) = ship(cx);
    let mut root = message(
        &channel(SHIP),
        "tavi",
        "Pilot laptop is free after lunch.",
        4,
    );
    root.id = Some("plain".into());
    inject(
        &hub,
        vec![
            Event::Line(root),
            Event::Line(reply("mira", "m1", "plain", 5)),
        ],
        cx,
    );
    settle(cx);
    cx.update(|window, _| {
        let link = window.find("irc-replies:plain");
        assert_eq!(
            link.label()
                .map(|label| label.ends_with("Opens the thread")),
            Some(true)
        );
    });
    cx.update(|window, cx| window.click("irc-replies:plain", cx));
    settle(cx);
    cx.update(|window, _| {
        assert!(window.find("irc-thread").visible());
        assert!(
            window.try_find("irc-thread-steps").is_none(),
            "not a request"
        );
        let replies = window.within("irc-thread-replies");
        assert!(replies.try_find_by_label("mira: Reply m1.").is_some());
    });
    inject(
        &hub,
        vec![Event::Line(reply("aurora-lyra", "m2", "m1", 6))],
        cx,
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, _| {
        let count = window.find("irc-replies:plain");
        assert!(
            count
                .label()
                .is_some_and(|label| label.starts_with("2 replies"))
        );
        assert!(window.find("inspector-section:Replies · 2").visible());
        let replies = window.within("irc-thread-replies");
        assert!(
            replies
                .try_find_by_label("aurora-lyra: Reply m2.")
                .is_some(),
            "live"
        );
    });
}

#[gpui_kit::test]
fn the_chosen_thread_survives_switching_tabs_and_conversations(cx: &mut TestAppContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    cx.update(|window, cx| window.click(("workspace-inspector-tab", 2usize), cx));
    settle(cx);
    let starbase = channel("#aurora-starbase");
    inject(
        &hub,
        vec![
            Event::Joined {
                buffer: starbase.clone(),
            },
            Event::Line(message(&starbase, "mira", "Morning.", 5)),
        ],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &starbase, cx));
    settle(cx);
    thread_tab(&board, cx);
    cx.update(|window, _| {
        assert!(
            window.find("irc-thread-prompt-line").visible(),
            "its own choice"
        );
    });
    board.update(cx, |board, cx| view::reveal(board, &channel(SHIP), cx));
    settle(cx);
    cx.update(|window, _| {
        assert!(window.find("irc-thread").visible(), "kept for the ship");
    });
}

#[gpui_kit::test]
fn without_the_reply_tag_the_footer_says_why(cx: &mut TestAppContext) {
    let (_, _, cx) = finished(cx, &[]);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    cx.update(|window, _| {
        assert!(window.try_find("irc-reply-editor").is_none());
        assert_eq!(
            window.find("irc-thread-reply-refused").label(),
            Some(NO_REPLY_TAG)
        );
    });
}
