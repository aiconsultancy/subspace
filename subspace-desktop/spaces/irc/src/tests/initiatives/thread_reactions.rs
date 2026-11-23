//! Reactions on the Thread tab as it is built: a reaction row under a
//! request leaves its linked status line and that line's status element
//! as they were, the tab shows the root's and the replies' pills
//! read-only, and reacting never lifts a gone reply's guard or sends it.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, Role, SharedString, TestAppContext, VisualTestContext};
use subspace_client_irc::{Command, Update};
use subspace_framework::dashboard::ChatInputEvent;

use super::super::remove::settle;
use super::thread::finished;
use super::thread_gone::gone_draft;
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

fn seq(hub: &Entity<Hub>, id: &str, cx: &mut VisualTestContext) -> i64 {
    hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&channel(SHIP))
            .and_then(|held| {
                held.entries
                    .iter()
                    .find(|entry| entry.line.id.as_deref() == Some(id))
            })
            .map(|entry| entry.seq)
            .expect("held")
    })
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
fn the_thread_tab_shows_the_root_s_and_the_replies_pills_read_only(cx: &mut TestAppContext) {
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
        for at in [ask1, r1, r2] {
            let pill = window
                .try_find(SharedString::from(format!(
                    "irc-thread-line:{at}:reaction:0:pill"
                )))
                .unwrap_or_else(|| panic!("the pill of {at} in the tab"));
            assert_ne!(pill.role(), Some(Role::Button), "read-only");
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
fn reacting_never_lifts_a_gone_reply_s_guard_or_sends_it(cx: &mut TestAppContext) {
    let (board, hub, cx) = reacted(cx);
    let conversation = gone_draft(&board, cx);
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    let r1 = seq(&hub, "r1", cx);
    cx.update(|window, cx| {
        window.click(SharedString::from(format!("irc-line:{r1}:reaction:0")), cx)
    });
    settle(cx);
    let input = conversation.read_with(cx, |conversation, _| conversation.composer().clone());
    input.update(cx, |_, cx| {
        cx.emit(ChatInputEvent::Submit("Ready for review.".into()))
    });
    settle(cx);
    let sent = hub.read_with(cx, |hub, _| hub.sent()[before..].to_vec());
    assert!(
        matches!(sent.as_slice(), [Command::React { msgid, .. }] if msgid == "r1"),
        "only the reaction went: {sent:?}"
    );
    conversation.read_with(cx, |conversation, _| {
        assert!(
            conversation
                .draft
                .reply
                .as_ref()
                .is_some_and(|reply| reply.gone && reply.root == "ask1"),
            "the guard holds"
        );
    });
}

#[gpui_kit::test]
fn r_opens_the_reaction_menu_from_reply_mode_and_keeps_the_reply(cx: &mut TestAppContext) {
    let (board, _hub, cx) = reacted(cx);
    cx.update(|_, cx| {
        subspace_framework::dashboard::chat_input::bind_keys(cx);
        crate::conversation::bind_keys(cx);
    });
    cx.update(|window, cx| window.click("irc-status:ask1", cx));
    settle(cx);
    cx.update(|window, cx| window.click("irc-thread-reply", cx));
    settle(cx);
    let conversation = super::thread::front(&board, cx);
    conversation.update_in(cx, |view, window, cx| {
        view.composer()
            .update(cx, |input, cx| input.focus(window, cx));
    });
    cx.simulate_keystrokes("shift-tab");
    cx.update(|window, cx| window.render_frame(cx));
    cx.simulate_keystrokes("r");
    cx.update(|window, cx| window.render_frame(cx));
    conversation.read_with(cx, |conversation, cx| {
        assert!(
            conversation.reacting.anchor.read(cx).is_open(),
            "the menu opens at the focused message"
        );
        assert!(conversation.reacting.notice.is_none());
        let reply = conversation.draft.reply.as_ref().unwrap();
        assert_eq!((reply.root.as_str(), reply.gone), ("ask1", false));
    });
}
