//! No reaction goes to a fleet agent in private: the controls are gone and
//! the one sending rule refuses it, as the composer refuses a message.
//! Reactions between people in private are ordinary.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Command, Event, Line, Update};

use super::super::inject;
use super::super::remove::settle;
use super::fleet::ship;
use crate::conversation::Conversation;
use crate::fixtures::{at, message, network_id, query};
use crate::hub::Hub;
use crate::{Irc, view};

fn with_id(mut line: Line, id: &str) -> Line {
    line.id = Some(id.into());
    line
}

/// `nick`'s private conversation, open in front, with their message `id`
/// and mira's 👍 on it, on a network that can react.
fn private(
    (board, hub): (&Entity<Irc>, &Entity<Hub>),
    nick: &str,
    id: &str,
    cx: &mut VisualTestContext,
) -> (BufferKey, Entity<Conversation>) {
    let buffer = query(nick);
    inject(
        hub,
        vec![Event::Line(with_id(
            message(&buffer, nick, "Hello.", 5),
            id,
        ))],
        cx,
    );
    let mut thumbs = subspace_client_irc::test_support::reaction(buffer.clone(), id, "👍", "mira");
    thumbs.time = at(6);
    let updates = vec![
        Update::ReactionSupport {
            network: network_id(),
            supported: true,
        },
        Update::Reaction(thumbs),
    ];
    hub.update(cx, |hub, cx| crate::hub::receive(hub, updates, at(7), cx));
    board.update(cx, |board, cx| view::reveal(board, &buffer, cx));
    settle(cx);
    cx.update(|window, cx| window.render_frame(cx));
    let conversation = board.read_with(cx, |board, _| board.dock.open[&buffer].entity.clone());
    (buffer, conversation)
}

fn sent(hub: &Entity<Hub>, cx: &mut VisualTestContext) -> usize {
    hub.read_with(cx, |hub, _| hub.sent().len())
}

#[gpui_kit::test]
fn no_reaction_goes_to_a_fleet_agent_in_private(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let (_, conversation) = private((&board, &hub), "aurora-lyra", "q1", cx);
    let seq = conversation.read_with(cx, |view, cx| {
        let held = view
            .hub
            .read(cx)
            .state
            .buffer(&query("aurora-lyra"))
            .unwrap();
        held.entries.back().unwrap().seq
    });
    cx.update(|window, _| {
        assert!(
            window
                .try_find(SharedString::from(format!(
                    "irc-line:{seq}:reaction:0:pill"
                )))
                .is_some(),
            "the pill is read-only"
        );
        assert!(
            window
                .try_find(SharedString::from(format!("irc-line:{seq}:add-reaction")))
                .is_none()
        );
    });
    let before = sent(&hub, cx);
    conversation.update(cx, |view, cx| {
        crate::conversation::reacting::toggle(view, ("q1".into(), "👍".into()), true, None, cx);
    });
    assert_eq!(
        sent(&hub, cx),
        before,
        "the rule refuses it where it is sent"
    );
    conversation.read_with(cx, |view, _| {
        let notice = view.reacting.notice.clone().unwrap_or_default();
        assert!(notice.contains("fleet agent"), "{notice}");
    });
}

#[gpui_kit::test]
fn people_react_to_each_other_in_private(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let (buffer, conversation) = private((&board, &hub), "tavi", "q2", cx);
    let seq = conversation.read_with(cx, |view, cx| {
        let held = view.hub.read(cx).state.buffer(&buffer).unwrap();
        held.entries.back().unwrap().seq
    });
    cx.update(|window, cx| {
        window.click(SharedString::from(format!("irc-line:{seq}:reaction:0")), cx)
    });
    settle(cx);
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::React {
                buffer: buffer.clone(),
                msgid: "q2".into(),
                reaction: "👍".into(),
            })
        );
    });
}

/// Reacts 👍 to `id` in aurora-lyra's private conversation; the
/// conversation and how many commands had been sent before.
fn react_privately(
    (board, hub): (&Entity<Irc>, &Entity<Hub>),
    id: &str,
    cx: &mut VisualTestContext,
) -> (Entity<Conversation>, usize) {
    let (_, conversation) = private((board, hub), "aurora-lyra", id, cx);
    let before = sent(hub, cx);
    conversation.update(cx, |view, cx| {
        crate::conversation::reacting::toggle(view, (id.into(), "👍".into()), true, None, cx);
    });
    (conversation, before)
}

#[gpui_kit::test]
fn no_private_reaction_goes_while_the_initiatives_could_not_be_read(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    // Another organisation, whose first read of the initiatives failed:
    // no one is known to be an agent, or known not to be.
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            catalog.change_scope_for_test(Some(99), cx);
            let failed = Err(subspace_api_client::Error::Network);
            catalog.set_memberships_for_test(Some("fixture-user".into()), failed, cx);
        })
    });
    settle(cx);
    let (conversation, before) = react_privately((&board, &hub), "q3", cx);
    assert_eq!(sent(&hub, cx), before, "nothing is sent");
    conversation.read_with(cx, |view, _| {
        assert_eq!(
            view.reacting.notice.as_deref(),
            Some(crate::reactions::gate::ROSTER_LOADING)
        );
    });
}

#[gpui_kit::test]
fn a_failed_refresh_after_a_good_one_closes_private_reactions_again(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    // The same account's next read answers NotFound: the initiatives,
    // and so who is an agent, are unknown again.
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            let gone = Err(subspace_api_client::Error::NotFound);
            catalog.set_memberships_for_test(Some("fixture-user".into()), gone, cx);
        })
    });
    settle(cx);
    let (conversation, before) = react_privately((&board, &hub), "q4", cx);
    assert_eq!(sent(&hub, cx), before, "nothing is sent to a known agent");
    conversation.read_with(cx, |view, _| {
        assert_eq!(
            view.reacting.notice.as_deref(),
            Some(crate::reactions::gate::ROSTER_LOADING)
        );
    });
}

#[gpui_kit::test]
fn a_hand_typed_private_tagmsg_meets_the_reaction_gate(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    hub.update(cx, |hub, cx| {
        hub.initiatives.catalog.clone().update(cx, |catalog, cx| {
            let gone = Err(subspace_api_client::Error::Network);
            catalog.change_scope_for_test(Some(98), cx);
            catalog.set_memberships_for_test(Some("fixture-user".into()), gone, cx);
        })
    });
    settle(cx);
    let (_, conversation) = private((&board, &hub), "tavi", "q5", cx);
    let input = conversation.read_with(cx, |view, _| view.composer().clone());
    let before = sent(&hub, cx);
    let raw = "/raw @+draft/react=\u{1F44D};+draft/reply=q5 TAGMSG tavi";
    crate::tests::completion::submit(&input, raw, cx);
    assert_eq!(
        sent(&hub, cx),
        before,
        "nothing is sent while the roster is unknown"
    );
    assert_eq!(
        input.read_with(cx, |input, _| input.error().map(str::to_owned)),
        Some(crate::reactions::gate::ROSTER_LOADING.to_owned())
    );
}
