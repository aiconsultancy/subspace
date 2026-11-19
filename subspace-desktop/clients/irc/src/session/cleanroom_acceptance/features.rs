//! ISUPPORT and conversation message acceptance tests.

use chrono::{TimeZone, Utc};

use super::{base, feed, key, lines, welcomed, wire};
use crate::model::{Event, LineKind, Target, Typing};
use crate::session::isupport::{Casemapping, Isupport};

#[test]
fn i01_defaults() {
    let isupport = Isupport::default();
    assert_eq!(isupport.chantypes, ['#', '&']);
    assert_eq!(isupport.prefix, [('o', '@'), ('v', '+')]);
    assert_eq!(isupport.symbols(), ['@', '+']);
    assert_eq!(isupport.network, None);
    assert_eq!(isupport.casemapping, Casemapping::Rfc1459);
    assert_eq!(isupport.chathistory, 0);
    assert!(isupport.statusmsg.is_empty());
    assert_eq!(isupport.list_modes, "beI");
    assert_eq!(isupport.param_modes, "k");
    assert_eq!(isupport.set_param_modes, "l");
    assert!(!isupport.whox);
}

#[test]
fn i02_tokens_update_parameters() {
    let mut isupport = Isupport::default();
    for token in [
        "PREFIX=(qaohv)~&@%+",
        "CHANTYPES=#!",
        r"NETWORK=Example\x20Net\x5Cwork",
        "chathistory=250",
        "STATUSMSG=@+",
        "CHANMODES=beI,k,lj,imnpst",
        "whox",
        "UNKNOWN=1",
    ] {
        isupport.apply(token);
    }
    assert_eq!(isupport.symbols(), ['~', '&', '@', '%', '+']);
    assert_eq!(isupport.prefix[0], ('q', '~'));
    assert_eq!(isupport.chantypes, ['#', '!']);
    assert_eq!(isupport.network.as_deref(), Some(r"Example Net\work"));
    assert_eq!(isupport.chathistory, 250);
    assert_eq!(isupport.statusmsg, ['@', '+']);
    assert_eq!(
        (isupport.list_modes.as_str(), isupport.param_modes.as_str()),
        ("beI", "k")
    );
    assert_eq!(isupport.set_param_modes, "lj");
    assert!(isupport.whox);
    isupport.apply("-WHOX");
    assert!(!isupport.whox);
    isupport.apply("DRAFT/CHATHISTORY=50");
    assert_eq!(isupport.chathistory, 50);
    isupport.apply("CHATHISTORY=lots");
    assert_eq!(isupport.chathistory, 0);
    isupport.apply("CHATHISTORY=70000");
    assert_eq!(isupport.chathistory, 0);
    isupport.apply("CHANMODES=b,k");
    assert_eq!(isupport.set_param_modes, "");
    for broken in ["PREFIX=", "PREFIX", "PREFIX=ov@+"] {
        let mut isupport = Isupport::default();
        isupport.apply(broken);
        assert!(isupport.prefix.is_empty(), "{broken}");
    }
}

#[test]
fn i03_case_mappings() {
    let mut isupport = Isupport::default();
    for (value, expected) in [
        ("ascii", Casemapping::Ascii),
        ("ASCII", Casemapping::Ascii),
        ("strict-rfc1459", Casemapping::StrictRfc1459),
        ("rfc1459", Casemapping::Rfc1459),
        ("rfc7613", Casemapping::Rfc1459),
    ] {
        isupport.apply(&format!("CASEMAPPING={value}"));
        assert_eq!(isupport.casemapping, expected, "{value}");
    }
    let name = r"Nick[A]\~ÉCOLE";
    assert_eq!(Casemapping::Ascii.fold(name), r"nick[a]\~École");
    assert_eq!(Casemapping::Rfc1459.fold(name), "nick{a}|^École");
    assert_eq!(Casemapping::StrictRfc1459.fold(name), "nick{a}|~École");
    isupport.apply("CASEMAPPING=ascii");
    assert_eq!(isupport.fold("#Ops[1]"), "#ops[1]");
}

#[test]
fn i04_channels_in_targets() {
    let mut isupport = Isupport::default();
    assert_eq!(isupport.channel("#ops"), Some("#ops"));
    assert_eq!(isupport.channel("&local"), Some("&local"));
    assert_eq!(isupport.channel("@#ops"), None, "no STATUSMSG yet");
    assert_eq!(isupport.channel("bob"), None);
    isupport.apply("STATUSMSG=@+");
    assert_eq!(isupport.channel("@+#ops"), Some("#ops"));
    assert_eq!(isupport.channel("+bob"), None);
    isupport.apply("CHANTYPES=");
    assert_eq!(isupport.channel("#ops"), None);
}

#[test]
fn g01_ctcp_replies_go_to_the_sender_and_never_to_ourselves() {
    let mut session = welcomed(base(), "echo-message");
    let out = feed(
        &mut session,
        &[
            ":lee!l@h.example PRIVMSG #room :\u{1}VERSION\u{1}",
            ":lee!l@h.example PRIVMSG kim :\u{1}PING\u{1}",
            ":lee!l@h.example PRIVMSG kim :\u{1}TIME\u{1}",
            ":lee!l@h.example PRIVMSG kim :\u{1}CLIENTINFO\u{1}",
            ":kim!k@h.example PRIVMSG lee :\u{1}VERSION\u{1}",
            ":lee!l@h.example NOTICE kim :\u{1}VERSION other 1.0\u{1}",
        ],
    );
    let sent = wire(&out);
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert!(sent[0].starts_with("NOTICE lee :\u{1}VERSION Subspace IRC "));
    assert!(sent[0].ends_with('\u{1}'));
    assert_eq!(sent[1], "NOTICE lee \u{1}PING\u{1}");
    assert_eq!(
        sent[2],
        "NOTICE lee :\u{1}TIME Fri, 7 Jun 2030 08:09:10 +0000\u{1}"
    );
    assert!(
        lines(&out).is_empty(),
        "CTCP other than ACTION makes no line"
    );
}

#[test]
fn g02_typing_from_others_only() {
    let mut session = welcomed(base(), "message-tags");
    let out = feed(
        &mut session,
        &[
            "@+typing=paused :lee!l@h.example TAGMSG #room",
            "@+typing=active :lee!l@h.example TAGMSG kim",
            "@+typing=bogus :lee!l@h.example TAGMSG #room",
            ":lee!l@h.example TAGMSG #room",
            "@+typing=active :kim!k@h.example TAGMSG #room",
        ],
    );
    assert_eq!(
        out.events,
        [
            Event::Typing {
                buffer: key(Target::Channel("#room".into())),
                nick: "lee".into(),
                state: Typing::Paused,
            },
            Event::Typing {
                buffer: key(Target::Query("lee".into())),
                nick: "lee".into(),
                state: Typing::Active,
            },
        ]
    );
}

#[test]
fn g03_read_markers_for_channels_and_nicks() {
    let mut session = welcomed(base(), "draft/read-marker");
    let out = feed(
        &mut session,
        &[
            ":s MARKREAD lee timestamp=2030-01-01T00:00:00.000Z",
            ":s MARKREAD #room timestamp=not-a-time",
            ":s MARKREAD #room",
        ],
    );
    assert_eq!(
        out.events,
        [Event::ReadMarker {
            buffer: key(Target::Query("lee".into())),
            time: Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).single().unwrap(),
        }]
    );
}

#[test]
fn g04_standard_replies_become_server_lines() {
    let mut session = welcomed(base(), "standard-replies");
    let out = feed(
        &mut session,
        &[
            ":s NOTE * OPER_MESSAGE :The server restarts at noon",
            ":s FAIL JOIN CHANNEL_FULL #room :Channel is full",
        ],
    );
    let lines = lines(&out);
    assert_eq!(lines[0].text, "* OPER_MESSAGE: The server restarts at noon");
    assert_eq!(lines[0].kind, LineKind::Info);
    assert_eq!(lines[1].text, "JOIN CHANNEL_FULL: Channel is full");
    assert_eq!(lines[1].kind, LineKind::Error);
    assert!(
        lines
            .iter()
            .all(|line| line.buffer.target == Target::Server)
    );
}

#[test]
fn g05_private_messages_open_a_query_that_takes_notices() {
    let mut session = welcomed(base(), "");
    let before = feed(&mut session, &[":lee!l@h.example NOTICE kim :before"]);
    assert_eq!(lines(&before)[0].buffer.target, Target::Server);
    let out = feed(
        &mut session,
        &[
            ":lee!l@h.example PRIVMSG kim :hello",
            ":LEE!l@h.example NOTICE kim :after",
        ],
    );
    assert_eq!(lines(&out)[0].buffer.target, Target::Query("lee".into()));
    assert_eq!(lines(&out)[1].buffer.target, Target::Query("lee".into()));
    assert_eq!(lines(&out)[1].kind, LineKind::Notice);
}
