//! Registration and nickname fallback acceptance tests.

use super::{Carry, Session, at, base, capabilities, feed, lines, net, welcomed, wire};
use crate::model::{Anchor, ConnectionStatus, Event, LineKind, NetworkConfig, Target};

#[test]
fn r01_the_burst_is_cap_ls_302_pass_nick_and_user() {
    let config = NetworkConfig {
        password: "pw".into(),
        username: "uid".into(),
        realname: "Kim Example".into(),
        ..base()
    };
    let mut session = Session::new(config, Carry::default());
    assert_eq!(
        wire(&session.start(at())),
        [
            "CAP LS 302",
            "PASS pw",
            "NICK kim",
            "USER uid 0 * :Kim Example"
        ]
    );
    let mut session = Session::new(base(), Carry::default());
    assert_eq!(
        wire(&session.start(at())),
        ["CAP LS 302", "NICK kim", "USER kim 0 * kim"]
    );
    assert!(!session.registered());
}

#[test]
fn r02_ping_is_answered_with_the_same_parameters() {
    let mut session = welcomed(base(), "");
    assert_eq!(
        wire(&feed(&mut session, &["PING irc.acceptance.test :t 1"])),
        ["PONG irc.acceptance.test :t 1"]
    );
}

#[test]
fn r03_nick_collisions_try_alternatives_then_underscores() {
    let config = NetworkConfig {
        alt_nicks: vec!["kim2".into()],
        ..base()
    };
    let mut session = Session::new(config, Carry::default());
    session.start(at());
    let steps = [
        (":s 433 * kim :Nickname is already in use", "NICK kim2"),
        (":s 432 * kim2 :Erroneous nickname", "NICK kim2_"),
        (":s 436 * kim2_ :Nickname collision", "NICK kim2__"),
        (":s 437 * kim2__ :Temporarily unavailable", "NICK kim2___"),
    ];
    for (line, expected) in steps {
        let out = feed(&mut session, &[line]);
        assert_eq!(wire(&out), [expected], "{line}");
        assert!(lines(&out).is_empty(), "no line before registration");
    }
    let out = feed(&mut session, &[":s 001 kim2___ :Hello"]);
    assert!(out.events.contains(&Event::OwnNick {
        network: net(),
        nick: "kim2___".into()
    }));
    let out = feed(
        &mut session,
        &[":s 433 kim2___ kim :Nickname is already in use"],
    );
    assert!(out.send.is_empty());
    let line = lines(&out)[0];
    assert_eq!(line.kind, LineKind::Error);
    assert_eq!(line.buffer.target, Target::Server);
    assert_eq!(line.text, "kim Nickname is already in use");
}

#[test]
fn r04_welcome_reports_then_subscribes_joins_and_resumes_queries() {
    let config = NetworkConfig {
        channels: vec!["#alpha key1".into(), "#beta".into()],
        ..base()
    };
    let carry = Carry {
        rejoin: vec!["#BETA".into(), "#gamma".into()],
        anchors: vec![Anchor {
            target: Target::Query("lee".into()),
            id: Some("m9".into()),
            time: at(),
        }],
        ..Carry::default()
    };
    let mut session = Session::new(config, carry);
    session.start(at());
    feed(
        &mut session,
        &[
            ":s CAP * LS :batch draft/metadata-2 draft/chathistory",
            ":s CAP kim ACK :batch draft/metadata-2 draft/chathistory",
        ],
    );
    let out = feed(&mut session, &[":s 001 kim :Welcome kim"]);
    assert_eq!(
        wire(&out),
        [
            "METADATA * SUB status",
            "JOIN #alpha key1",
            "JOIN #beta",
            "JOIN #gamma",
            "CHATHISTORY LATEST lee msgid=m9 500",
        ]
    );
    let kinds: Vec<&str> = out
        .events
        .iter()
        .map(|event| match event {
            Event::OwnNick { .. } => "own",
            Event::Status { .. } => "status",
            Event::Line(_) => "line",
            Event::Capabilities { .. } => "caps",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["own", "status", "line"]);
    assert!(out.events.contains(&Event::Status {
        network: net(),
        status: ConnectionStatus::Connected { nick: "kim".into() }
    }));
    assert_eq!(lines(&out)[0].text, "Welcome kim");
    assert_eq!(lines(&out)[0].kind, LineKind::Info);
    assert!(session.registered());
}

#[test]
fn r05_a_welcome_without_negotiation_reports_no_capabilities_and_sends_no_cap_end() {
    let mut session = Session::new(base(), Carry::default());
    session.start(at());
    let out = feed(&mut session, &[":legacy.example 001 kim :Hi"]);
    assert_eq!(capabilities(&out), [Vec::<String>::new()]);
    assert!(!wire(&out).contains(&"CAP END".to_string()));
    let line = feed(&mut session, &["ERROR :Closing Link: kim (Bye)"]);
    assert_eq!(lines(&line)[0].text, "Closing Link: kim (Bye)");
    assert_eq!(lines(&line)[0].kind, LineKind::Error);
}

#[test]
fn r06_the_welcome_takes_the_nick_the_server_assigns() {
    let mut session = Session::new(base(), Carry::default());
    session.start(at());
    let out = feed(&mut session, &[":s 001 kim_truncated :Hi"]);
    assert!(out.events.contains(&Event::OwnNick {
        network: net(),
        nick: "kim_truncated".into()
    }));
}
