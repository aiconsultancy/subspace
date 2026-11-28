//! Reactions on the Thread tab as it is built: a reaction row under a
//! request leaves its linked status line and that line's status element
//! as they were, the tab shows the root's and the replies' pills
//! read-only, and reacting keeps the inspector draft separate.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Role, SharedString, TestAppContext, VisualTestContext};
use subspace_client_irc::{Command, Update};

use super::super::remove::settle;
use super::thread::{finished, front, reply_input, seq};
use crate::fixtures::{at, channel, network_id};
use crate::hub::Hub;
use crate::thread::MESSAGE_TAGS;

const SHIP: &str = "#aurora-lyra";

fn reaction(msgid: &str, value: &str, nick: &str) -> Update {
    let mut reaction =
        subspace_client_irc::test_support::reaction(channel(SHIP), msgid, value, nick);
    reaction.time = at(50);
    Update::Reaction(reaction)
}

fn receive(hub: &Entity<Hub>, updates: Vec<Update>, cx: &mut VisualTestContext) {
    hub.update(cx, |hub, cx| crate::hub::receive(hub, updates, at(51), cx));
    settle(cx);
    cx.update(|window, cx| window.render_frame(cx));
}

/// The ship's thread with reaction support and a reaction on the request
/// and on each reply.
fn reacted(cx: &mut TestAppContext) -> (Entity<crate::Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = finished(cx, &[MESSAGE_TAGS]);
    receive(
        &hub,
        vec![
            Update::ReactionSupport {
                network: network_id(),
                supported: true,
            },
            reaction("ask1", "👍", "mira"),
            reaction("r1", "👀", "mira"),
            reaction("r2", "🎉", "aurora-lyra"),
        ],
        cx,
    );
    (board, hub, cx)
}

#[gpui_kit::test]
fn a_reaction_row_leaves_the_linked_status_line_and_its_status_element(cx: &mut TestAppContext) {
    let (_board, hub, cx) = reacted(cx);
    let ask1 = seq(&hub, "ask1", cx);
    cx.update(|window, _| {
        assert!(
            window
                .try_find(SharedString::from(format!("irc-line:{ask1}:reaction:0")))
                .is_some(),
            "the request's pill shows"
        );
        assert_eq!(window.find("irc-status:ask1").role(), Some(Role::Button));
        assert_eq!(
            window.find("irc-status:ask1:live").role(),
            Some(Role::Status),
            "the status element stays beside the linked line"
        );
    });
}

#[gpui_kit::test]
fn the_thread_tab_keeps_the_root_pill_read_only_and_replies_interactive(cx: &mut TestAppContext) {
    let (_board, hub, cx) = reacted(cx);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    let (ask1, r1, r2) = (
        seq(&hub, "ask1", cx),
        seq(&hub, "r1", cx),
        seq(&hub, "r2", cx),
    );
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        let root = window.find(format!("irc-thread-line:{ask1}:reaction:0:pill"));
        assert_ne!(
            root.role(),
            Some(Role::Button),
            "the channel also shows this pill"
        );
        for at in [r1, r2] {
            let pill = window.find(format!("irc-line:{at}:reaction:0"));
            assert_eq!(
                pill.role(),
                Some(Role::Button),
                "reply reactions stay interactive"
            );
        }
    });
    let sent = hub.read_with(cx, |hub, _| hub.sent().len());
    cx.update(|window, cx| {
        window.click(
            SharedString::from(format!("irc-thread-line:{ask1}:reaction:0:pill")),
            cx,
        )
    });
    settle(cx);
    assert_eq!(
        hub.read_with(cx, |hub, _| hub.sent().len()),
        sent,
        "selecting a pill in the tab sends nothing"
    );
}

#[gpui_kit::test]
fn reacting_keeps_the_independent_reply_draft(cx: &mut TestAppContext) {
    let (board, hub, cx) = reacted(cx);
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    let input = reply_input(&front(&board, cx), "ask1", cx);
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_text("Ready for review.", window, cx)
        })
    });
    settle(cx);
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    let r1 = seq(&hub, "r1", cx);
    cx.update(|window, cx| {
        window.click(SharedString::from(format!("irc-line:{r1}:reaction:0")), cx)
    });
    settle(cx);
    let sent = hub.read_with(cx, |hub, _| hub.sent()[before..].to_vec());
    assert!(matches!(sent.as_slice(), [Command::React { msgid, .. }] if msgid == "r1"));
    input.read_with(cx, |input, cx| {
        assert_eq!(input.text(cx), "Ready for review.")
    });
}
