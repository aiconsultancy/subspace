//! Request status lines on the real transcript: trusted progress folds into
//! one line under each request and updates in place, with a clock and a
//! activity mark; a lookalike from another account, questions and final
//! replies stay messages; the line goes stale while the network is down and
//! settles into `Done in …` when the request ends.

use chrono::Local;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext, Window};
use subspace_client_irc::{BufferKey, ConnectionStatus, Event, Line, LineKind, Member, Typing};

use super::{channel_events, inject, mount, settings};
use crate::fixtures::{at, channel, line, member, message, network_id};
use crate::hub::Hub;
use crate::state::Buffer;
use crate::thread_status::PROGRESS_TAG;
use crate::{Irc, view};

/// The fixture ship's account, as the server reports it: named after its
/// nick, as fleet servers require.
const SHIP: &str = "astra";

fn joined(nick: &str, account: &str) -> Event {
    let mut member: Member = member(nick, "");
    member.account = Some(account.into());
    Event::MemberJoined {
        buffer: channel("#subspace"),
        member,
    }
}

/// Our request `id` to astra, in the thread of `root` when one is given.
fn request(room: &BufferKey, id: &str, root: Option<&str>, minute: i64) -> Line {
    let mut line = message(
        room,
        "subspace",
        &format!("astra: please handle {id}"),
        minute,
    );
    line.id = Some(id.into());
    line.reply_to = root.map(str::to_owned);
    line
}

/// A tagged notice from `nick` (logged in to `account`) on `request`.
fn notice(
    (nick, account): (&str, &str),
    room: &BufferKey,
    request: &str,
    value: &str,
    minute: i64,
) -> Line {
    let mut line = line(room, LineKind::Notice, nick, "Routine progress.", minute);
    line.id = Some(format!("{nick}-{request}-{minute}"));
    line.account = Some(account.into());
    line.reply_to = Some(request.into());
    line.client_tags.insert(PROGRESS_TAG.into(), value.into());
    line
}

fn progress(room: &BufferKey, request: &str, value: &str, minute: i64) -> Line {
    notice(("astra", SHIP), room, request, value, minute)
}

/// astra's visible message `id`, a reply to `request` when one is given.
fn said(room: &BufferKey, request: Option<&str>, id: &str, minute: i64) -> Line {
    let mut line = message(room, "astra", &format!("Message {id}"), minute);
    line.id = Some(id.into());
    line.account = Some(SHIP.into());
    line.reply_to = request.map(str::to_owned);
    line
}

fn status(window: &Window, request: &str) -> Option<String> {
    window
        .try_find(format!("irc-status:{request}"))
        .and_then(|found| found.label().map(str::to_owned))
}

fn local(minute: i64) -> String {
    at(minute).with_timezone(&Local).format("%H:%M").to_string()
}

fn stale(label: &str, minute: i64) -> String {
    format!(
        "astra: {label}. Stale: the last update was at {}",
        local(minute)
    )
}

/// Whether the line with message ID `id` has a transcript row.
fn has_row(window: &Window, held: &Buffer, id: &str) -> bool {
    held.entries
        .iter()
        .find(|entry| entry.line.id.as_deref() == Some(id))
        .is_some_and(|entry| {
            window
                .try_find(format!("irc-line:{}:row", entry.seq))
                .is_some()
        })
}

fn status_event(status: ConnectionStatus) -> Event {
    Event::Status {
        network: network_id(),
        status,
    }
}

/// Opens `#subspace` holding three requests to astra, the third in the
/// first one's thread, with trusted progress on each and a lookalike.
fn open_with_requests(board: &Entity<Irc>, hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    let room = channel("#subspace");
    let mut events = channel_events();
    events.extend([joined("astra", SHIP), joined("nova", "nova")]);
    events.extend(
        [
            request(&room, "first", None, 4),
            request(&room, "second", None, 5),
            request(&room, "third", Some("first"), 6),
            progress(&room, "first", "received", 7),
            progress(&room, "second", "queued", 8),
            progress(&room, "third", "submitting", 9),
            progress(&room, "first", "tool", 10),
            notice(("nova", "nova"), &room, "first", "completed", 11),
        ]
        .map(Event::Line),
    );
    inject(hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
}

/// One line per request, the trusted notices folded out of the rows and
/// the reply count, and the lookalike still a notice.
fn assert_one_line_per_request(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    let room = channel("#subspace");
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            status(window, "first").as_deref(),
            Some("astra: Using a tool")
        );
        assert_eq!(status(window, "second").as_deref(), Some("astra: Queued"));
        assert!(window.find("irc-status:first:activity").visible());
        assert!(window.try_find("irc-status:second:activity").is_none());
        assert!(window.try_find("irc-status:first:meter").is_none());
        assert!(window.find("irc-status:first:clock").visible());
        assert_eq!(
            status(window, "third").as_deref(),
            Some("astra: Receipt unconfirmed"),
            "a second request in the same thread has its own line"
        );
        let held = hub.read(cx).state.buffer(&room).unwrap();
        for id in [
            "astra-first-7",
            "astra-second-8",
            "astra-third-9",
            "astra-first-10",
        ] {
            assert!(!has_row(window, held, id), "{id} is folded");
        }
        assert!(
            has_row(window, held, "nova-first-11"),
            "the lookalike stays a notice"
        );
        assert_eq!(
            window.find("irc-replies:first").label(),
            Some(format!("2 replies · Last reply {}", local(11)).as_str()),
            "the follow-up and the lookalike count; folded progress does not"
        );
    });
}

/// A question on the second request changes only its line and stays a
/// message, and typing still shows above the composer.
fn ask_a_question(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    let room = channel("#subspace");
    inject(
        hub,
        vec![
            Event::Line(progress(&room, "second", "waiting", 12)),
            Event::Line(said(&room, Some("second"), "question", 13)),
            Event::Typing {
                buffer: room.clone(),
                nick: "astra".into(),
                state: Typing::Active,
            },
        ],
        cx,
    );
    cx.update(|window, cx| {
        assert_eq!(
            status(window, "first").as_deref(),
            Some("astra: Using a tool"),
            "an update to the second request leaves the first alone"
        );
        assert_eq!(
            status(window, "second").as_deref(),
            Some("astra: Waiting for your answer")
        );
        let held = hub.read(cx).state.buffer(&room).unwrap();
        assert!(
            has_row(window, held, "question"),
            "the question stays a message"
        );
        assert!(
            held.typing.contains_key("astra"),
            "typing still shows above the composer"
        );
    });
}

/// While reconnecting, the first request's line shows its last stage as
/// stale.
fn lose_the_connection(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    let reconnecting = ConnectionStatus::Reconnecting {
        attempt: 1,
        delay_secs: 5,
        error: String::new(),
    };
    inject(hub, vec![status_event(reconnecting)], cx);
    cx.update(|window, _| {
        assert_eq!(status(window, "first"), Some(stale("Using a tool", 10)));
        assert!(window.try_find("irc-status:first:stale").is_some());
    });
}

/// After reconnecting, a main-room final reply clears nothing; the second
/// request's `completed` clears only its own line.
fn finish_the_second_request(hub: &Entity<Hub>, cx: &mut VisualTestContext) {
    let room = channel("#subspace");
    let connected = ConnectionStatus::Connected {
        nick: "subspace".into(),
    };
    inject(
        hub,
        vec![
            status_event(connected),
            Event::Line(progress(&room, "second", "reply", 14)),
            Event::Line(said(&room, None, "final", 15)),
        ],
        cx,
    );
    cx.update(|window, cx| {
        assert_eq!(
            status(window, "second").as_deref(),
            Some("astra: Writing a reply"),
            "a main-room final reply clears nothing by itself"
        );
        let held = hub.read(cx).state.buffer(&room).unwrap();
        assert!(has_row(window, held, "final"));
    });
    inject(
        hub,
        vec![Event::Line(progress(&room, "second", "completed", 16))],
        cx,
    );
}

#[gpui_kit::test]
fn trusted_progress_folds_into_one_line_per_request(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    open_with_requests(&board, &hub, cx);
    assert_one_line_per_request(&hub, cx);
    ask_a_question(&hub, cx);
    lose_the_connection(&hub, cx);
    finish_the_second_request(&hub, cx);
    cx.update(|window, _| {
        assert_eq!(
            status(window, "second").as_deref(),
            Some("astra: Done in 11 m"),
            "the matching request settled"
        );
        assert!(
            window.try_find("irc-status:second:clock").is_none(),
            "a settled line has no clock"
        );
        assert_eq!(
            status(window, "first"),
            Some(stale("Using a tool", 10)),
            "the first is still in progress, seen before the reconnect"
        );
        assert_eq!(
            status(window, "third"),
            Some(stale("Receipt unconfirmed", 9))
        );
    });
}

#[gpui_kit::test]
fn untrusted_progress_stays_a_notice_without_a_status_line(cx: &mut TestAppContext) {
    let (board, hub, cx) = mount(cx, settings());
    let room = channel("#subspace");
    let mut events = channel_events();
    events.push(joined("nova", "nova"));
    let mut unvouched = progress(&room, "first", "tool", 6);
    unvouched.account = None;
    events.extend(
        [
            request(&room, "first", None, 4),
            notice(("nova", "nova"), &room, "first", "tool", 5),
            unvouched,
            notice(("astra", "impostor"), &room, "first", "reply", 7),
        ]
        .map(Event::Line),
    );
    inject(&hub, events, cx);
    board.update(cx, |board, cx| view::reveal(board, &room, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            status(window, "first"),
            None,
            "neither notice comes from the account named astra"
        );
        let held = hub.read(cx).state.buffer(&room).unwrap();
        assert!(has_row(window, held, "nova-first-5"));
        assert!(has_row(window, held, "astra-first-6"));
        assert!(has_row(window, held, "astra-first-7"));
    });
}

mod clock;
