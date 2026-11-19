//! Buffer identity under the server's case mapping, canonical member
//! spellings, answers to history requests that cannot be sent, and user
//! mode lines.

use super::super::{Carry, Session};
use super::{
    ALL_CAPS, ERGO_ISUPPORT, PLAIN_CAPS, buffer, channel, command, config, feed, in_channel, lines,
    now, registered, sent,
};
use crate::model::{BufferKey, Command, Event, LineKind, NetworkId, Target};

fn keys(events: &[Event]) -> Vec<BufferKey> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Line(line) => Some(line.buffer.clone()),
            Event::History { buffer, .. }
            | Event::Joined { buffer }
            | Event::Left { buffer }
            | Event::Topic { buffer, .. }
            | Event::Members { buffer, .. }
            | Event::MemberJoined { buffer, .. }
            | Event::MemberLeft { buffer, .. }
            | Event::MemberChanged { buffer, .. }
            | Event::Typing { buffer, .. }
            | Event::ReadMarker { buffer, .. } => Some(buffer.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_event_uses_the_first_spelling_of_a_channel() {
    let mut session = registered(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":subspace!u@h JOIN #Room * :Subspace",
            ":ergo.test 332 subspace #ROOM :Topic",
            ":ergo.test 353 subspace = #room :@alice!a@h subspace!u@h",
            ":ergo.test 366 subspace #room :End of NAMES list",
            ":alice!a@h PRIVMSG #room :hello",
            "@+typing=active :alice!a@h TAGMSG #ROOM",
            ":bob!b@h JOIN #room * :Bob",
            ":bob!b@h PART #rOOm",
            ":subspace!u@h MARKREAD #room timestamp=2026-01-02T03:04:05.000Z",
            ":ergo.test BATCH +h chathistory #room",
            "@batch=h;msgid=old :alice!a@h PRIVMSG #room :older",
            ":ergo.test BATCH -h",
        ],
    );
    let keys = keys(&out.events);
    assert!(keys.len() > 10);
    assert!(keys.iter().all(|key| *key == channel("#Room")), "{keys:?}");
    let history = out.events.iter().find_map(|event| match event {
        Event::History { lines, .. } => Some(lines.clone()),
        _ => None,
    });
    assert_eq!(history.unwrap()[0].buffer, channel("#Room"));
    let typed = command(
        &mut session,
        Command::Input {
            buffer: channel("#ROOM"),
            text: "/msg #rooM hi".into(),
        },
    );
    assert_eq!(sent(&typed), ["PRIVMSG #rooM hi"]);
}

#[test]
fn rfc1459_folds_brackets_into_one_buffer_and_query() {
    let mut session = registered(PLAIN_CAPS);
    feed(
        &mut session,
        &[":ergo.test 005 subspace CASEMAPPING=rfc1459 :are supported"],
    );
    let out = feed(
        &mut session,
        &[
            ":subspace!u@h JOIN #[a]~",
            ":x!x@h PRIVMSG #{A}^ :same channel",
            ":Bob[1]!b@h PRIVMSG subspace :hi",
            ":bob{1}!b@h PRIVMSG subspace :again",
        ],
    );
    let lines = lines(&out);
    assert_eq!(lines[1].buffer, channel("#[a]~"));
    assert_eq!(lines[2].buffer, buffer(Target::Query("Bob[1]".into())));
    assert_eq!(lines[3].buffer, buffer(Target::Query("Bob[1]".into())));
    let opened = command(
        &mut session,
        Command::Query {
            network: NetworkId("local".into()),
            nick: "BOB{1}".into(),
        },
    );
    assert!(opened.events.contains(&Event::Joined {
        buffer: buffer(Target::Query("Bob[1]".into()))
    }));
}

#[test]
fn spellings_survive_a_reconnect() {
    let mut session = registered(PLAIN_CAPS);
    feed(&mut session, &[":subspace!u@h JOIN #Room"]);
    let carry = session.into_carry();
    assert_eq!(carry.rejoin, ["#Room"]);
    let mut next = Session::new(config(), carry);
    next.start(now());
    let out = feed(
        &mut next,
        &[
            ":ergo.test 001 subspace :Welcome",
            ERGO_ISUPPORT,
            ":subspace!u@h JOIN #room",
        ],
    );
    assert!(out.events.contains(&Event::Joined {
        buffer: channel("#Room")
    }));
}

#[test]
fn member_events_use_the_stored_spelling() {
    let mut session = registered(PLAIN_CAPS);
    feed(
        &mut session,
        &[
            ":ergo.test 005 subspace CASEMAPPING=rfc1459 :are supported",
            ":subspace!u@h JOIN #c",
            ":ergo.test 353 subspace = #c :[bob] carol dave subspace",
            ":ergo.test 366 subspace #c :End of NAMES list",
        ],
    );
    let kick = feed(&mut session, &[":op!o@h KICK #c {BOB} :out"]);
    assert!(kick.events.contains(&Event::MemberLeft {
        buffer: channel("#c"),
        nick: "[bob]".into(),
    }));
    let quit = feed(&mut session, &[":CAROL!c@h QUIT :gone"]);
    assert!(quit.events.contains(&Event::MemberLeft {
        buffer: channel("#c"),
        nick: "carol".into(),
    }));
    let renamed = feed(&mut session, &[":DAVE!d@h NICK david"]);
    assert!(renamed.events.iter().any(|event| matches!(
        event,
        Event::MemberChanged { old_nick, member, .. } if old_nick == "dave" && member.nick == "david"
    )));
}

#[test]
fn history_without_chathistory_is_answered_as_complete() {
    let mut session = registered("batch server-time");
    let request = Command::LoadHistory {
        buffer: channel("#c"),
        before: Some(now()),
        limit: 100,
    };
    let out = command(&mut session, request.clone());
    assert!(sent(&out).is_empty());
    assert_eq!(
        out.events,
        [Event::History {
            buffer: channel("#c"),
            lines: Vec::new(),
            complete: true
        }]
    );
    let mut early = Session::new(config(), Carry::default());
    early.start(now());
    let before_registration = command(&mut early, request);
    assert!(matches!(
        before_registration.events.as_slice(),
        [Event::History {
            complete: false,
            ..
        }]
    ));
}

#[test]
fn user_modes_read_as_sentences() {
    let mut session = in_channel(PLAIN_CAPS);
    let texts: Vec<(LineKind, Target, String)> = [
        ":ergo.test 221 subspace +Zi",
        ":subspace MODE subspace :+w",
        ":alice!a@h MODE #c +m",
    ]
    .into_iter()
    .flat_map(|line| {
        let out = feed(&mut session, &[line]);
        lines(&out)
            .into_iter()
            .map(|line| (line.kind, line.buffer.target.clone(), line.text.clone()))
            .collect::<Vec<_>>()
    })
    .collect();
    assert_eq!(
        texts,
        [
            (
                LineKind::Info,
                Target::Server,
                "Your user modes are +Zi".into()
            ),
            (
                LineKind::Info,
                Target::Server,
                "Your user modes changed: +w".into()
            ),
            (
                LineKind::Mode,
                Target::Channel("#c".into()),
                "sets mode +m".into()
            ),
        ]
    );
}
