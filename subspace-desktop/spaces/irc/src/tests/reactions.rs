//! Reactions on the real transcript: pills under a message, toggling ours
//! only once the client reports it, stop reactions on a running request,
//! refusals, support changes and removing the conversation.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, SharedString, TestAppContext, VisualTestContext, Window};
use subspace_client_irc::{BufferKey, Command, Event, Line, LineKind, ReactionError, Update};

use super::{channel_events, inject, mount, settings};
use crate::fixtures::{at, channel, line, member, message, network_id};
use crate::hub::Hub;
use crate::thread_status::PROGRESS_TAG;
use crate::{Irc, view};

mod gate;
mod take_back;

pub(super) fn room() -> BufferKey {
    channel("#subspace")
}

pub(super) fn with_id(mut line: Line, id: &str) -> Line {
    line.id = Some(id.into());
    line
}

pub(super) fn reaction(msgid: &str, value: &str, nick: &str) -> Update {
    let mut reaction = subspace_client_irc::test_support::reaction(room(), msgid, value, nick);
    reaction.own = nick == "subspace";
    reaction.time = at(20);
    Update::Reaction(reaction)
}

pub(super) fn support(supported: bool) -> Update {
    Update::ReactionSupport {
        network: network_id(),
        supported,
    }
}

pub(super) fn receive(hub: &Entity<Hub>, updates: Vec<Update>, cx: &mut VisualTestContext) {
    hub.update(cx, |hub, cx| crate::hub::receive(hub, updates, at(30), cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.render_frame(cx));
}

/// `#subspace` with alice's message `m1`, our request `r1` to astra with
/// her progress, and support on.
pub(super) fn open(cx: &mut TestAppContext) -> (Entity<Irc>, Entity<Hub>, &mut VisualTestContext) {
    let (board, hub, cx) = mount(cx, settings());
    let mut astra = member("astra", "");
    astra.account = Some("astra".into());
    let mut events = channel_events();
    let mut progress = line(&room(), LineKind::Notice, "astra", "Working.", 5);
    progress.id = Some("p1".into());
    progress.account = Some("astra".into());
    progress.reply_to = Some("r1".into());
    progress
        .client_tags
        .insert(PROGRESS_TAG.into(), "tool".into());
    events.push(Event::MemberJoined {
        buffer: room(),
        member: astra,
    });
    events.extend(
        [
            with_id(message(&room(), "alice", "ship it", 3), "m1"),
            with_id(
                message(&room(), "subspace", "astra: please deploy", 4),
                "r1",
            ),
            progress,
        ]
        .map(Event::Line),
    );
    inject(&hub, events, cx);
    receive(&hub, vec![support(true)], cx);
    board.update(cx, |board, cx| view::reveal(board, &room(), cx));
    cx.run_until_parked();
    (board, hub, cx)
}

pub(super) fn seq(hub: &Entity<Hub>, id: &str, cx: &mut VisualTestContext) -> i64 {
    hub.read_with(cx, |hub, _| {
        hub.state
            .buffer(&room())
            .and_then(|held| {
                held.entries
                    .iter()
                    .find(|entry| entry.line.id.as_deref() == Some(id))
            })
            .map(|entry| entry.seq)
            .expect("held")
    })
}

pub(super) fn label(window: &Window, id: &str) -> Option<String> {
    window
        .try_find(SharedString::from(id.to_owned()))
        .and_then(|found| found.label().map(str::to_owned))
}

#[gpui_kit::test]
fn pills_show_who_reacted_and_toggle_ours_only_when_reported(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    receive(
        &hub,
        vec![
            reaction("m1", "👍", "bob"),
            reaction("m1", "👍", "subspace"),
            reaction("m1", "🎉", "carol"),
        ],
        cx,
    );
    let m1 = seq(&hub, "m1", cx);
    let thumbs = format!("irc-line:{m1}:reaction:0");
    let party = format!("irc-line:{m1}:reaction:1");
    cx.update(|window, _| {
        assert_eq!(
            label(window, &thumbs).as_deref(),
            Some("👍 from bob and you")
        );
        assert_eq!(label(window, &party).as_deref(), Some("🎉 from carol"));
        assert!(
            window
                .try_find(format!("irc-line:{m1}:add-reaction"))
                .is_some()
        );
    });
    cx.update(|window, cx| window.click(SharedString::from(thumbs.clone()), cx));
    hub.read_with(cx, |hub, _| {
        assert_eq!(
            hub.sent().last(),
            Some(&Command::Unreact {
                buffer: room(),
                msgid: "m1".into(),
                reaction: "👍".into(),
            }),
            "selecting our pill takes ours back"
        );
    });
    cx.update(|window, cx| window.click(SharedString::from(party.clone()), cx));
    cx.update(|window, cx| window.render_frame(cx));
    cx.update(|window, _| {
        assert_eq!(
            label(window, &party).as_deref(),
            Some("🎉 from carol"),
            "nothing shows as ours before the client reports it"
        );
    });
    receive(&hub, vec![reaction("m1", "🎉", "subspace")], cx);
    cx.update(|window, _| {
        assert_eq!(
            label(window, &party).as_deref(),
            Some("🎉 from carol and you")
        );
    });
}

#[gpui_kit::test]
fn a_stop_pill_on_a_running_request_says_what_it_asked_and_sends_nothing(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    receive(&hub, vec![reaction("r1", "\u{270B}", "bob")], cx);
    let r1 = seq(&hub, "r1", cx);
    let pill = format!("irc-line:{r1}:reaction:0");
    cx.update(|window, _| {
        assert_eq!(
            label(window, &pill).as_deref(),
            Some(
                "\u{270B} from bob. It asked astra to stop; the status line shows whether it did."
            )
        );
    });
    let before = hub.read_with(cx, |hub, _| hub.sent().len());
    cx.update(|window, cx| window.click(SharedString::from(pill.clone()), cx));
    cx.update(|window, cx| window.render_frame(cx));
    hub.read_with(cx, |hub, _| {
        assert_eq!(hub.sent().len(), before, "no stray stop")
    });
    cx.update(|window, _| {
        let notice = label(window, "irc-reaction-notice").unwrap_or_default();
        assert!(notice.contains("Stop astra"), "{notice}");
    });
    let m1 = seq(&hub, "m1", cx);
    receive(&hub, vec![reaction("m1", "\u{270B}", "bob")], cx);
    cx.update(|window, _| {
        assert_eq!(
            label(window, &format!("irc-line:{m1}:reaction:0")).as_deref(),
            Some("\u{270B} from bob"),
            "elsewhere an ordinary reaction"
        );
    });
}

#[gpui_kit::test]
fn refusals_support_and_removal(cx: &mut TestAppContext) {
    let (_board, hub, cx) = open(cx);
    receive(&hub, vec![reaction("m1", "👍", "bob")], cx);
    let m1 = seq(&hub, "m1", cx);
    receive(
        &hub,
        vec![Update::ReactionRefused {
            buffer: room(),
            msgid: "m1".into(),
            error: ReactionError::NotConnected,
        }],
        cx,
    );
    cx.update(|window, _| {
        assert_eq!(
            label(window, "irc-reaction-notice").as_deref(),
            Some("Reaction not sent: not connected.")
        );
    });
    receive(&hub, vec![support(false)], cx);
    cx.update(|window, _| {
        assert!(
            window
                .try_find(format!("irc-line:{m1}:add-reaction"))
                .is_none()
        );
        assert_eq!(
            label(window, &format!("irc-line:{m1}:reaction:0:pill")).as_deref(),
            Some("👍 from bob"),
            "known reactions stay, read-only"
        );
    });
    let held = hub.read_with(cx, |hub, _| {
        hub.state.buffer(&room()).cloned().expect("held")
    });
    hub.update(cx, |hub, cx| hub.remove(&room(), cx));
    hub.read_with(cx, |hub, _| {
        assert!(
            hub.reactions.shown(&room(), &held).is_empty(),
            "a removed conversation's reactions go with it"
        );
    });
}
