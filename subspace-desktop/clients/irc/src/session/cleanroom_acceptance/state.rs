//! Batch and reconnect state acceptance tests.

use chrono::{TimeZone, Utc};

use super::{Carry, Session, at, base, feed, key, lines, net, welcomed, wire};
use crate::model::{Anchor, Command, Event, Target};
use crate::session::configure_autojoin;

#[test]
fn b01_uncollected_batches_pass_through_as_they_arrive() {
    let mut session = welcomed(base(), "batch labeled-response");
    let opened = feed(&mut session, &["@label=q1 :s BATCH +lr labeled-response"]);
    assert!(opened.events.is_empty());
    let inner = feed(&mut session, &["@batch=lr :s NOTICE kim :inside"]);
    assert_eq!(lines(&inner)[0].text, "inside");
    assert!(feed(&mut session, &[":s BATCH -lr"]).events.is_empty());
    let stray = feed(&mut session, &["@batch=nope :s NOTICE kim :unknown batch"]);
    assert_eq!(lines(&stray)[0].text, "unknown batch");
    assert!(
        feed(&mut session, &[":s BATCH -never-opened"])
            .events
            .is_empty()
    );
}

#[test]
fn b02_a_multiline_batch_becomes_one_message_with_the_opening_tags() {
    let mut session = welcomed(base(), "batch draft/multiline message-tags");
    let out = feed(
        &mut session,
        &[
            "@msgid=open1;time=2030-02-03T04:05:06.000Z :lee!l@h.example BATCH +ml draft/multiline #room",
            "@batch=ml;msgid=part1;time=2001-01-01T00:00:00.000Z :lee!l@h.example PRIVMSG #room :alpha",
            "@batch=ml;draft/multiline-concat :lee!l@h.example PRIVMSG #room :beta",
            "@batch=ml :lee!l@h.example PRIVMSG #room :",
            "@batch=ml :lee!l@h.example PRIVMSG #room :gamma",
        ],
    );
    assert!(out.events.is_empty(), "nothing before the batch closes");
    let out = feed(&mut session, &[":lee!l@h.example BATCH -ml"]);
    let lines = lines(&out);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "alphabeta\n\ngamma");
    assert_eq!(lines[0].id.as_deref(), Some("open1"));
    assert_eq!(
        lines[0].time,
        Utc.with_ymd_and_hms(2030, 2, 3, 4, 5, 6).single().unwrap()
    );
    assert_eq!(lines[0].buffer.target, Target::Channel("#room".into()));
    let empty = feed(
        &mut session,
        &[
            ":lee!l@h.example BATCH +e draft/multiline #room",
            ":lee!l@h.example BATCH -e",
        ],
    );
    assert!(empty.events.is_empty());
}

#[test]
fn b03_batches_inside_a_collected_batch_join_their_parent() {
    let mut session = welcomed(base(), "batch draft/chathistory draft/multiline");
    let out = feed(
        &mut session,
        &[
            ":s BATCH +h chathistory #room",
            "@batch=h :s BATCH +x example.org/other",
            "@batch=x;msgid=n1 :lee!l@h.example PRIVMSG #room :nested",
            ":s BATCH -x",
            "@batch=h;msgid=n2 :lee!l@h.example PRIVMSG #room :direct",
            ":s BATCH -h",
        ],
    );
    let history: Vec<(&str, bool)> = out
        .events
        .iter()
        .filter_map(|event| match event {
            Event::History {
                lines, complete, ..
            } => Some(
                lines
                    .iter()
                    .map(move |line| (line.text.as_str(), *complete)),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    // Unrequested history is complete only when empty.
    assert_eq!(history, [("nested", false), ("direct", false)]);
    assert!(lines(&out).is_empty());
}

#[test]
fn b04_bouncer_playback_marks_lines_as_replayed() {
    let mut session = welcomed(base(), "batch");
    feed(
        &mut session,
        &[
            ":kim!k@h JOIN #room",
            ":s 353 kim = #room :kim",
            ":s 366 kim #room :End of /NAMES list.",
        ],
    );
    let out = feed(
        &mut session,
        &[
            ":bnc.example BATCH +p znc.in/playback #room",
            "@batch=p :lee!l@h.example JOIN #room",
            "@batch=p :lee!l@h.example PRIVMSG #room :back",
            ":bnc.example BATCH -p",
        ],
    );
    let replayed: Vec<&str> = out
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Replayed(line) => Some(line.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(replayed, ["joined", "back"]);
    assert!(lines(&out).is_empty());
    assert!(
        out.events
            .iter()
            .any(|event| matches!(event, Event::MemberJoined { .. }))
    );
}

#[test]
fn x01_buffer_spellings_survive_into_the_next_connection() {
    let carry = Carry {
        spellings: vec!["#Room".into()],
        ..Carry::default()
    };
    let mut session = Session::new(base(), carry);
    session.start(at());
    feed(&mut session, &[":s 001 kim :Hi"]);
    let out = feed(&mut session, &[":kim!k@h JOIN #ROOM"]);
    assert!(out.events.contains(&Event::Joined {
        buffer: key(Target::Channel("#Room".into()))
    }));
    assert_eq!(session.channel_names(), ["#ROOM"]);
    let carry = session.into_carry();
    assert_eq!(carry.rejoin, ["#ROOM"]);
    assert_eq!(carry.spellings, ["#Room"]);
}

#[test]
fn x02_carry_forget_keeps_removals_once_and_the_newest_anchor() {
    let mut carry = Carry::default();
    let forget = |name: &str| Command::Forget {
        buffer: key(Target::Channel(name.into())),
    };
    carry.forget(&forget("#Ops"));
    carry.forget(&forget("#OPS"));
    carry.forget(&Command::Forget {
        buffer: key(Target::Query("lee".into())),
    });
    assert_eq!(carry.left, ["#Ops"]);
    let anchor = |seconds: i64| Anchor {
        target: Target::Channel("#ops".into()),
        id: None,
        time: at() + chrono::Duration::seconds(seconds),
    };
    for seconds in [5, 1, 9, 3] {
        carry.forget(&Command::Anchors {
            network: net(),
            anchors: vec![anchor(seconds)],
        });
    }
    assert_eq!(carry.anchors, [anchor(9)]);
    let before = carry.clone();
    assert!(configure_autojoin(&mut carry, &base(), &forget("#x")).is_empty());
    assert_eq!(carry, before);
}

#[test]
fn x03_disconnect_quits_with_the_reason() {
    let mut session = welcomed(base(), "");
    let out = session.command(
        Command::Disconnect {
            network: net(),
            reason: "see you".into(),
        },
        at(),
    );
    assert_eq!(wire(&out), ["QUIT :see you"]);
    assert!(out.quit);
    assert_eq!(session.network(), net());
}

#[test]
fn x04_abandon_ends_requests_and_clears_statuses_once() {
    let mut session = welcomed(base(), "batch draft/chathistory draft/metadata-2");
    feed(
        &mut session,
        &[
            ":kim!k@h JOIN #room",
            ":s 761 kim lee status * :away for lunch",
        ],
    );
    let events = session.abandon();
    assert!(events.contains(&Event::History {
        buffer: key(Target::Channel("#room".into())),
        lines: Vec::new(),
        complete: false,
    }));
    assert!(events.contains(&Event::NickStatus {
        network: net(),
        nick: "lee".into(),
        status: None,
    }));
    assert!(session.abandon().is_empty());
}

#[test]
fn x05_written_events_move_anchors() {
    let mut session = welcomed(base(), "");
    feed(&mut session, &[":kim!k@h JOIN #room"]);
    let out = session.command(
        Command::Say {
            buffer: key(Target::Channel("#room".into())),
            text: "hi".into(),
            action: false,
        },
        at(),
    );
    assert_eq!(wire(&out), ["PRIVMSG #room hi"]);
    let Some(Event::Line(echo)) = out.events.first() else {
        panic!("a local echo without echo-message");
    };
    assert!(echo.own);
    let mut written = echo.clone();
    written.buffer = key(Target::Query("lee".into()));
    written.time = at() + chrono::Duration::seconds(30);
    session.written(&[Event::Line(written)]);
    let anchors = session.into_carry().anchors;
    let targets: Vec<&Target> = anchors.iter().map(|anchor| &anchor.target).collect();
    assert_eq!(
        targets,
        [
            &Target::Channel("#room".into()),
            &Target::Query("lee".into())
        ]
    );
}
