mod all_or_nothing;
mod ctcp;
mod oversized;
mod paragraphs;

use chrono::{TimeZone, Utc};

use super::super::{Carry, Session};
use super::{
    ALL_CAPS, PLAIN_CAPS, buffer, channel, command, config, feed, in_channel, lines, now,
    registered, sent,
};
use crate::model::{Command, Event, LineKind, Target, Typing};

fn input(session: &mut Session, target: Target, text: &str) -> super::Output {
    command(
        session,
        Command::Input {
            buffer: buffer(target),
            text: text.into(),
        },
    )
}

#[test]
fn messages_carry_time_id_account_and_highlights() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@time=2024-05-06T07:08:09.123Z;msgid=abc;account=alice :alice!a@h PRIVMSG #c :hey Subspace, see this",
            ":bob!b@h PRIVMSG #c :subspacer is not us, but deploy is a word",
            ":bob!b@h PRIVMSG #c :nothing here",
            ":bob!b@h PRIVMSG #c :\u{1}ACTION waves\u{1}",
        ],
    );
    let lines = lines(&out);
    assert_eq!(lines[0].id.as_deref(), Some("abc"));
    assert_eq!(lines[0].account.as_deref(), Some("alice"));
    let expected = Utc.with_ymd_and_hms(2024, 5, 6, 7, 8, 9).single().unwrap()
        + chrono::Duration::milliseconds(123);
    assert_eq!(lines[0].time, expected);
    assert!(lines[0].highlight);
    assert!(lines[1].highlight);
    assert!(!lines[2].highlight);
    assert_eq!(lines[2].time, now());
    assert_eq!(lines[3].kind, LineKind::Action);
    assert_eq!(lines[3].text, "waves");
}

#[test]
fn private_messages_open_queries_and_early_notices_go_to_the_server() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    let early = feed(
        &mut session,
        &[":NickServ!s@services NOTICE subspace :Welcome"],
    );
    assert_eq!(lines(&early)[0].buffer.target, Target::Server);
    let mut session = registered(PLAIN_CAPS);
    let out = feed(
        &mut session,
        &[
            ":bob!b@h PRIVMSG subspace :hi there",
            ":bob!b@h NOTICE subspace :notice in query",
            ":stranger!s@h NOTICE subspace :notice to server",
            ":ergo.test NOTICE * :server notice",
            ":alice!a@h PRIVMSG @#c :ops only",
        ],
    );
    let lines = lines(&out);
    assert_eq!(lines[0].buffer.target, Target::Query("bob".into()));
    assert_eq!(lines[1].buffer.target, Target::Query("bob".into()));
    assert_eq!(lines[1].kind, LineKind::Notice);
    assert_eq!(lines[2].buffer.target, Target::Server);
    assert_eq!(lines[3].buffer.target, Target::Server);
    assert_eq!(lines[4].buffer, channel("#c"));
}

#[test]
fn echo_message_marks_own_lines_without_local_echo() {
    let mut session = in_channel(ALL_CAPS);
    let out = input(&mut session, Target::Channel("#c".into()), "hello");
    assert_eq!(sent(&out), ["PRIVMSG #c hello"]);
    assert!(lines(&out).is_empty());
    let echo = feed(&mut session, &["@msgid=m1 :subspace!u@h PRIVMSG #c hello"]);
    assert!(lines(&echo)[0].own);
    assert!(!lines(&echo)[0].highlight);
    let query = feed(&mut session, &[":subspace!u@h PRIVMSG bob :to bob"]);
    assert_eq!(lines(&query)[0].buffer.target, Target::Query("bob".into()));
}

#[test]
fn local_echo_without_echo_message() {
    let mut session = in_channel(PLAIN_CAPS);
    let out = input(&mut session, Target::Channel("#c".into()), "/me waves");
    assert_eq!(sent(&out), ["PRIVMSG #c :\u{1}ACTION waves\u{1}"]);
    let line = lines(&out)[0];
    assert!(line.own);
    assert_eq!(line.kind, LineKind::Action);
    assert_eq!(line.from.as_deref(), Some("subspace"));
}

#[test]
fn say_sends_literal_text_and_actions_but_never_commands() {
    let mut session = in_channel(PLAIN_CAPS);
    let mut say = |target: Target, text: &str, action: bool| {
        let buffer = buffer(target);
        let text = text.to_owned();
        command(
            &mut session,
            Command::Say {
                buffer,
                text,
                action,
            },
        )
    };
    let out = say(Target::Channel("#c".into()), "/quit now", false);
    assert_eq!(sent(&out), ["PRIVMSG #c :/quit now"]);
    assert_eq!(lines(&out)[0].text, "/quit now");
    let out = say(Target::Channel("#c".into()), "waves", true);
    assert_eq!(sent(&out), ["PRIVMSG #c :\u{1}ACTION waves\u{1}"]);
    let out = say(Target::Server, "hello", false);
    assert!(out.send.is_empty());
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
}

#[test]
fn multiline_input_uses_a_batch_when_available() {
    let mut session = in_channel(ALL_CAPS);
    let out = input(&mut session, Target::Channel("#c".into()), "one\n\ntwo");
    assert_eq!(
        sent(&out),
        [
            "BATCH +subspace1 draft/multiline #c",
            "@batch=subspace1 PRIVMSG #c one",
            "@batch=subspace1 PRIVMSG #c :",
            "@batch=subspace1 PRIVMSG #c two",
            "BATCH -subspace1",
        ]
    );
    let mut session = in_channel(PLAIN_CAPS);
    let out = input(&mut session, Target::Channel("#c".into()), "one\n\ntwo");
    assert_eq!(sent(&out), ["PRIVMSG #c one", "PRIVMSG #c two"]);
    assert_eq!(lines(&out).len(), 2);
}

#[test]
fn long_input_is_split_to_fit() {
    let mut session = in_channel(PLAIN_CAPS);
    let text = "word ".repeat(200);
    let out = input(&mut session, Target::Channel("#c".into()), text.trim());
    assert!(out.send.len() > 1);
    assert!(sent(&out).iter().all(|line| line.len() + 2 <= 512));
    let mut session = in_channel(ALL_CAPS);
    let out = input(
        &mut session,
        Target::Channel("#c".into()),
        &format!("{}\nend", "x".repeat(600)),
    );
    let sent = sent(&out);
    assert!(
        sent.iter()
            .any(|line| line.starts_with("@batch=subspace1;draft/multiline-concat "))
    );
}

#[test]
fn receives_multiline_batches_as_one_line() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@msgid=ml1;time=2024-01-01T00:00:00Z :alice!a@h BATCH +m draft/multiline #c",
            "@batch=m :alice!a@h PRIVMSG #c :first",
            "@batch=m :alice!a@h PRIVMSG #c :second",
            "@batch=m;draft/multiline-concat :alice!a@h PRIVMSG #c : half",
            ":alice!a@h BATCH -m",
        ],
    );
    let lines = lines(&out);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "first\nsecond half");
    assert_eq!(lines[0].id.as_deref(), Some("ml1"));
}

#[test]
fn typing_markread_and_standard_replies() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            "@+typing=active :bob!b@h TAGMSG #c",
            "@+typing=done :subspace!u@h TAGMSG #c",
            ":ergo.test MARKREAD #c timestamp=2024-01-01T00:00:00.000Z",
            ":ergo.test MARKREAD #c *",
            ":ergo.test FAIL CHATHISTORY INVALID_TARGET #nope :Messages could not be retrieved",
            ":ergo.test WARN REHASH CERTS_EXPIRED :Certificate expired",
        ],
    );
    assert_eq!(
        out.events[0],
        Event::Typing {
            buffer: channel("#c"),
            nick: "bob".into(),
            state: Typing::Active,
        }
    );
    assert!(matches!(&out.events[1], Event::ReadMarker { buffer, .. } if *buffer == channel("#c")));
    let lines = lines(&out);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].kind, LineKind::Error);
    assert_eq!(
        lines[0].text,
        "CHATHISTORY INVALID_TARGET: Messages could not be retrieved"
    );
    assert_eq!(lines[1].kind, LineKind::Info);

    let typing = command(
        &mut session,
        Command::Typing {
            buffer: channel("#c"),
            state: Typing::Paused,
        },
    );
    assert_eq!(sent(&typing), ["@+typing=paused TAGMSG #c"]);
    let mark = command(
        &mut session,
        Command::MarkRead {
            buffer: channel("#c"),
            time: now(),
        },
    );
    assert_eq!(
        sent(&mark),
        ["MARKREAD #c timestamp=2026-01-02T03:04:05.000Z"]
    );
}

#[test]
fn slash_commands_become_protocol_messages() {
    let mut session = in_channel(PLAIN_CAPS);
    let here = Target::Channel("#c".into());
    let cases = [
        ("/join other", "JOIN #other"),
        ("/part bye now", "PART #c :bye now"),
        ("/topic New topic", "TOPIC #c :New topic"),
        ("/kick bob be nice", "KICK #c bob :be nice"),
        ("/mode +o bob", "MODE #c +o bob"),
        ("/invite bob", "INVITE bob #c"),
        ("/whois bob", "WHOIS bob"),
        ("/away lunch", "AWAY lunch"),
        ("/raw PRIVMSG #c :raw text", "PRIVMSG #c :raw text"),
        ("/foo bar baz", "FOO bar baz"),
        ("/msg bob hi there", "PRIVMSG bob :hi there"),
        ("/nick other", "NICK other"),
        ("//slash", "PRIVMSG #c /slash"),
    ];
    for (text, expected) in cases {
        let out = input(&mut session, here.clone(), text);
        assert_eq!(
            sent(&out).last().map(String::as_str),
            Some(expected),
            "{text}"
        );
    }
    let quit = input(&mut session, here, "/quit done");
    assert_eq!(sent(&quit), ["QUIT done"]);
    assert!(quit.quit);
}

#[test]
fn input_errors_become_error_lines() {
    let mut session = registered(PLAIN_CAPS);
    let out = input(&mut session, Target::Server, "hello");
    assert!(out.send.is_empty());
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
    let out = input(&mut session, Target::Server, "/part");
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
    let out = input(&mut session, Target::Server, "/msg bob");
    assert_eq!(lines(&out)[0].text, "Usage: /msg <nick> <text>");
    let query = input(&mut session, Target::Server, "/query bob hi");
    assert!(query.events.contains(&Event::Joined {
        buffer: buffer(Target::Query("bob".into()))
    }));
    assert_eq!(
        sent(&query),
        ["CHATHISTORY LATEST bob * 100", "PRIVMSG bob hi"]
    );
}
