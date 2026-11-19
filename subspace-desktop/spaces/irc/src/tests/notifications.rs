//! Notifications through the real board and hub, with the board's declared
//! defaults as the shell installs them and a sink that records each request.

use gpui_kit::{Entity, Global, TestAppContext, VisualTestContext};
use subspace_client_irc::{BufferKey, Event, Line};
use subspace_framework::dashboard::Dashboard as _;
use subspace_framework::dashboard::notifications::{self, Notice};

use super::{channel_events, inject, mount, settings};
use crate::dock::{self, Where};
use crate::fixtures::{channel, message, query};
use crate::{Irc, view};

#[derive(Default)]
struct Delivered(Vec<(u64, Notice)>);
impl Global for Delivered {}

/// Installs IRC's declared defaults and records what reaches the shell.
fn listen(cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let declared =
            <Irc as subspace_framework::dashboard::Dashboard>::settings().notification_defaults;
        notifications::install_defaults([(crate::ID, declared)], cx);
        cx.set_global(Delivered::default());
        cx.set_global(notifications::Sink(|_, target, notice, cx| {
            cx.global_mut::<Delivered>().0.push((target, notice));
        }));
    });
}

fn delivered(cx: &mut VisualTestContext) -> Vec<(u64, Notice)> {
    cx.update(|_, cx| cx.global::<Delivered>().0.clone())
}

/// A message the runtime marked as mentioning us, as it does for `@subspace`.
fn tagged(buffer: &BufferKey, from: &str, text: &str, minute: i64) -> Line {
    let mut line = message(buffer, from, text, minute);
    line.highlight = true;
    line
}

/// Mounts the board on #subspace, connected, with the window in front, and
/// starts listening once the setup's own lines are in.
fn reading_subspace(cx: &mut TestAppContext) -> (Entity<Irc>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    inject(&hub, channel_events(), cx);
    board.update(cx, |board, cx| {
        view::reveal(board, &channel("#subspace"), cx)
    });
    cx.run_until_parked();
    listen(cx);
    (board, cx)
}

fn say(board: &Entity<Irc>, line: Line, cx: &mut VisualTestContext) {
    let hub = board.read_with(cx, |board, _| board.hub.clone());
    if let Some(hub) = hub {
        inject(&hub, vec![Event::Line(line)], cx);
    }
}

#[gpui_kit::test]
fn a_tag_in_another_channel_notifies_with_who_and_where(cx: &mut TestAppContext) {
    let (board, cx) = reading_subspace(cx);
    let ops = channel("#ops");
    say(&board, message(&ops, "alice", "deploy finished", 5), cx);
    assert!(delivered(cx).is_empty(), "no tag, no notification");
    say(
        &board,
        tagged(&ops, "alice", "@subspace can you look?", 6),
        cx,
    );
    let notices: Vec<Notice> = delivered(cx)
        .into_iter()
        .map(|(_, notice)| notice)
        .collect();
    assert_eq!(
        notices,
        [Notice::described(
            "alice mentioned you in #ops",
            "Local Ergo"
        )],
        "on by default, naming the sender, the channel and the network"
    );
}

#[gpui_kit::test]
fn the_conversation_in_view_with_the_app_in_front_stays_quiet(cx: &mut TestAppContext) {
    let (board, cx) = reading_subspace(cx);
    let room = channel("#subspace");
    say(
        &board,
        tagged(&room, "alice", "@subspace right here", 5),
        cx,
    );
    say(
        &board,
        message(&room, "subspace", "@subspace own echo", 6),
        cx,
    );
    assert!(delivered(cx).is_empty());
    let carol = query("carol");
    say(&board, message(&carol, "carol", "are you there?", 7), cx);
    let notices: Vec<Notice> = delivered(cx)
        .into_iter()
        .map(|(_, notice)| notice)
        .collect();
    assert_eq!(
        notices,
        [Notice::described(
            "carol sent you a private message",
            "Local Ergo"
        )],
        "a private message elsewhere still notifies"
    );
}

#[gpui_kit::test]
fn a_conversation_shown_beside_stays_quiet_and_a_tab_behind_notifies(cx: &mut TestAppContext) {
    let (board, cx) = reading_subspace(cx);
    let (room, ops, crew) = (channel("#subspace"), channel("#ops"), channel("#crew"));
    say(&board, message(&ops, "dave", "standing by", 2), cx);
    say(&board, message(&crew, "dave", "standing by", 3), cx);
    for (buffer, place) in [
        (&ops, Where::Right),
        (&room, Where::Tab),
        (&crew, Where::Tab),
        (&room, Where::Tab),
    ] {
        let buffer = buffer.clone();
        board.update(cx, |board, cx| dock::request(board, buffer, place, cx));
        cx.run_until_parked();
    }
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&room));
    });
    say(&board, tagged(&ops, "alice", "@subspace beside you", 5), cx);
    assert!(delivered(cx).is_empty(), "#ops is shown beside #subspace");
    say(
        &board,
        tagged(&crew, "alice", "@subspace behind you", 6),
        cx,
    );
    let notices: Vec<Notice> = delivered(cx)
        .into_iter()
        .map(|(_, notice)| notice)
        .collect();
    assert_eq!(
        notices,
        [Notice::described(
            "alice mentioned you in #crew",
            "Local Ergo"
        )],
        "#crew is a tab behind #subspace"
    );
    // Closing the split changes what is on screen without moving focus.
    cx.update(|window, cx| {
        board.update(cx, |board, cx| dock::close(board, &ops, window, cx));
    });
    cx.run_until_parked();
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&room));
    });
    say(&board, tagged(&ops, "alice", "@subspace now closed", 7), cx);
    assert_eq!(delivered(cx).len(), 2, "#ops is no longer on screen");
}

#[gpui_kit::test]
fn in_the_background_the_conversation_in_view_notifies_and_choosing_it_returns(
    cx: &mut TestAppContext,
) {
    let (board, cx) = reading_subspace(cx);
    let room = channel("#subspace");
    let carol = query("carol");
    cx.deactivate_window();
    say(
        &board,
        tagged(&room, "alice", "@subspace while you were away", 5),
        cx,
    );
    say(&board, message(&carol, "carol", "ping", 6), cx);
    let targets: Vec<u64> = delivered(cx)
        .into_iter()
        .map(|(target, _)| target)
        .collect();
    assert_eq!(targets.len(), 2, "the app is not in front");
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board.open_notification(targets[0], window, cx)
        });
        window.activate_window();
    });
    cx.run_until_parked();
    board.read_with(cx, |board, cx| {
        assert_eq!(board.dock.focused.as_ref(), Some(&room));
        let hub = board.hub.as_ref().map(|hub| hub.read(cx));
        let held = hub.and_then(|hub| hub.state.buffer(&room));
        assert_eq!(held.map(|buffer| buffer.unread), Some(0), "reading it");
    });
    cx.update(|window, cx| {
        board.update(cx, |board, cx| {
            board.open_notification(targets[1], window, cx)
        });
    });
    cx.run_until_parked();
    board.read_with(cx, |board, _| {
        assert_eq!(board.dock.focused.as_ref(), Some(&carol));
    });
}
