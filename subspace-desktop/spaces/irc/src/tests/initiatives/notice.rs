//! Every other way a private message could still reach an agent: an
//! attachment, a target list, a raw line, and a notification while another
//! network shows; and **Go to** only for a channel that exists.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Command, Event, NetworkId, Target};

use super::super::inject;
use super::super::remove::settle;
use super::direct::{NOTICE, SHIP, front, no_query, notice, sent_input, submit};
use super::fleet::ship;
use crate::fixtures::{channel, message, query};
use crate::hub::Hub;
use crate::{Irc, view};

/// Whether the notice offers **Go to**.
fn go_offered(cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| window.try_find("irc-agent-notice-go").is_some())
}

/// An old private conversation with the captain, then closed, so a
/// notification for it would open it again.
fn closed_old_query(
    board: &Entity<Irc>,
    hub: &Entity<Hub>,
    cx: &mut VisualTestContext,
) -> BufferKey {
    let old = query("aurora-lyra");
    inject(
        hub,
        vec![Event::Line(super::fleet::signed(message(
            &old,
            "aurora-lyra",
            "From before the rule.",
            9,
        )))],
        cx,
    );
    board.update(cx, |board, cx| crate::remove::remove(board, &old, cx));
    settle(cx);
    old
}

/// Something of another network in front, as when another network shows.
fn elsewhere_in_front(board: &Entity<Irc>, cx: &mut VisualTestContext) {
    board.update(cx, |board, _| {
        board.dock.focused = Some(BufferKey {
            network: NetworkId("elsewhere".into()),
            target: Target::Channel("#elsewhere".into()),
        });
    });
}

#[gpui_kit::test]
fn an_attachment_to_an_agent_starts_no_upload(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let old = query("aurora-lyra");
    inject(
        &hub,
        vec![Event::Line(super::fleet::signed(message(
            &old,
            "aurora-lyra",
            "From before the rule.",
            9,
        )))],
        cx,
    );
    board.update(cx, |board, cx| view::reveal(board, &old, cx));
    settle(cx);
    let conversation = front(&board, &old, cx);
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("plan.txt");
    std::fs::write(&path, "contents").unwrap();
    conversation.update(cx, |view, cx| {
        crate::uploads::outgoing::paths(view, vec![path], cx)
    });
    cx.run_until_parked();
    for text in ["look", ""] {
        submit(&conversation, text, cx);
        conversation.read_with(cx, |view, _| {
            assert!(view.tray.idle(), "{text:?}: no upload started");
            assert_eq!(view.tray.items.len(), 1, "the tray stays");
        });
        assert!(!sent_input(&hub, cx));
        assert_eq!(notice(cx).as_deref(), Some(NOTICE));
    }
    hub.read_with(cx, |hub, _| {
        assert!(
            !hub.sent()
                .iter()
                .any(|sent| matches!(sent, Command::SendAll { .. })),
            "nothing is sent"
        );
    });
}

#[gpui_kit::test]
fn a_target_list_or_a_raw_line_to_an_agent_keeps_the_draft(cx: &mut TestAppContext) {
    let (board, hub, cx) = ship(cx);
    let conversation = front(&board, &channel(SHIP), cx);
    for text in [
        "/msg mira,aurora-lyra are you free?",
        "/notice #aurora-lyra,Aurora-Lyra heads up",
        "/raw PRIVMSG aurora-lyra :hello",
        "/quote NOTICE aurora-lyra :hello",
        "/privmsg aurora-lyra hello",
    ] {
        submit(&conversation, text, cx);
        assert!(!sent_input(&hub, cx), "{text} is not sent");
        assert!(
            notice(cx).is_some_and(|said| said.contains("ask in #aurora-lyra")),
            "{text}"
        );
        cx.update(|window, cx| window.click("irc-agent-notice-dismiss", cx));
        settle(cx);
    }
    submit(
        &conversation,
        "/raw PRIVMSG #aurora-lyra :to the channel",
        cx,
    );
    assert!(sent_input(&hub, cx), "a channel is never refused");
}

#[gpui_kit::test]
fn a_notification_while_another_network_shows_reveals_the_agent_s_channel_with_the_notice(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = ship(cx);
    let old = closed_old_query(&board, &hub, cx);
    elsewhere_in_front(&board, cx);
    board.update(cx, |board, cx| view::show_notified(board, &old, cx));
    settle(cx);
    no_query(&hub, "aurora-lyra", cx);
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&channel(SHIP)));
    });
    assert_eq!(notice(cx).as_deref(), Some(NOTICE));
    assert!(go_offered(cx), "the channel is in the list");
}

#[gpui_kit::test]
fn without_the_agent_s_channel_the_notice_shows_in_the_server_buffer_without_go_to(
    cx: &mut TestAppContext,
) {
    let (board, hub, cx) = ship(cx);
    let old = closed_old_query(&board, &hub, cx);
    board.update(cx, |board, cx| {
        crate::remove::remove(board, &channel(SHIP), cx)
    });
    settle(cx);
    elsewhere_in_front(&board, cx);
    board.update(cx, |board, cx| view::show_notified(board, &old, cx));
    settle(cx);
    no_query(&hub, "aurora-lyra", cx);
    let server = BufferKey {
        network: crate::fixtures::network_id(),
        target: Target::Server,
    };
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&server));
    });
    assert_eq!(notice(cx).as_deref(), Some(NOTICE));
    assert!(
        !go_offered(cx),
        "#aurora-lyra is a guessed name the list does not hold: joining it would create it"
    );
}
