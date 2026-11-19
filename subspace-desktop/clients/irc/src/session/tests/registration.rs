use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::super::{Carry, Session};
use super::{ERGO_ISUPPORT, ERGO_LS, config, feed, lines, now, sent};
use crate::model::{ConnectionStatus, Event, LineKind, NetworkId, Sasl};
use crate::proto::parse;

const REQUESTED: &str = "CAP REQ :account-notify account-tag away-notify batch cap-notify chghost draft/chathistory draft/event-playback draft/multiline draft/read-marker echo-message extended-join invite-notify labeled-response message-tags multi-prefix server-time setname standard-replies userhost-in-names sasl";

#[test]
fn nickname_fallback_allows_an_alternative_and_three_underscores() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    let rejected = parse(":srv 433 * subspace :Nickname in use").unwrap();
    for next in ["subspace2", "subspace2_", "subspace2__", "subspace2___"] {
        let retry = session.receive(rejected.clone(), now());
        assert_eq!(sent(&retry), [format!("NICK {next}")]);
        assert!(retry.fatal.is_none());
    }
    let exhausted = session.receive(rejected, now());
    assert!(exhausted.send.is_empty());
    assert_eq!(
        exhausted.fatal.as_deref(),
        Some("No IRC nickname was accepted. Nickname in use")
    );
    assert!(!session.registered());
}

#[test]
fn registers_with_cap_ls_302_and_sasl_plain() {
    let mut session = Session::new(config(), Carry::default());
    let start = session.start(now());
    assert_eq!(
        sent(&start),
        ["CAP LS 302", "NICK subspace", "USER subspace 0 * subspace"]
    );

    // The first LS line is a continuation: nothing is requested yet.
    assert!(feed(&mut session, &[ERGO_LS[0]]).send.is_empty());
    assert_eq!(sent(&feed(&mut session, &[ERGO_LS[1]])), [REQUESTED]);

    let acked = REQUESTED.trim_start_matches("CAP REQ :");
    let ack = feed(
        &mut session,
        &[&format!(":ergo.test CAP subspace ACK :{acked}")],
    );
    assert_eq!(sent(&ack), ["AUTHENTICATE PLAIN"]);

    let challenge = feed(&mut session, &["AUTHENTICATE +"]);
    let payload = STANDARD.encode("subspace\0subspace\0secret");
    assert_eq!(sent(&challenge), [format!("AUTHENTICATE {payload}")]);

    let done = feed(
        &mut session,
        &[
            ":ergo.test 900 * * subspace :You are now logged in as subspace",
            ":ergo.test 903 * :Authentication successful",
        ],
    );
    assert_eq!(sent(&done), ["CAP END"]);
    let capabilities = done.events.iter().find_map(|event| match event {
        Event::Capabilities { enabled, .. } => Some(enabled.clone()),
        _ => None,
    });
    assert!(capabilities.is_some_and(|enabled| enabled.contains(&"sasl".to_string())));
    assert_eq!(lines(&done).len(), 2);

    let welcome = feed(
        &mut session,
        &[
            ":ergo.test 001 subspace :Welcome to the ErgoTest IRC Network subspace",
            ERGO_ISUPPORT,
        ],
    );
    assert_eq!(sent(&welcome), ["JOIN #subspace"]);
    assert!(welcome.events.contains(&Event::OwnNick {
        network: NetworkId("local".into()),
        nick: "subspace".into(),
    }));
    assert!(welcome.events.contains(&Event::Status {
        network: NetworkId("local".into()),
        status: ConnectionStatus::Connected {
            nick: "subspace".into()
        },
    }));
    assert!(session.registered());
}

#[test]
fn chunks_long_sasl_payloads() {
    let mut config = config();
    // 300 bytes of credentials encode to exactly 400 bytes of base64.
    config.sasl = Some(Sasl {
        username: "u".repeat(100),
        password: "p".repeat(98),
        password_file: None,
        external: false,
    });
    let mut session = Session::new(config, Carry::default());
    session.start(now());
    feed(
        &mut session,
        &[":srv CAP * LS :sasl", ":srv CAP * ACK :sasl"],
    );
    let out = feed(&mut session, &["AUTHENTICATE +"]);
    let sent = sent(&out);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].len(), "AUTHENTICATE ".len() + 400);
    assert_eq!(sent[1], "AUTHENTICATE +");
}

#[test]
fn sasl_external_and_failure_still_end_negotiation() {
    let mut config = config();
    config.sasl = Some(Sasl {
        username: String::new(),
        password: String::new(),
        password_file: None,
        external: true,
    });
    let mut session = Session::new(config, Carry::default());
    session.start(now());
    let ls = feed(&mut session, &[":srv CAP * LS :sasl=EXTERNAL"]);
    assert_eq!(sent(&ls), ["CAP REQ sasl"]);
    let ack = feed(&mut session, &[":srv CAP * ACK :sasl"]);
    assert_eq!(sent(&ack), ["AUTHENTICATE EXTERNAL"]);
    assert_eq!(
        sent(&feed(&mut session, &["AUTHENTICATE +"])),
        ["AUTHENTICATE +"]
    );
    let failed = feed(&mut session, &[":srv 904 * :SASL authentication failed"]);
    assert_eq!(sent(&failed), ["CAP END"]);
    assert_eq!(lines(&failed)[0].kind, LineKind::Error);
}

#[test]
fn skips_sasl_when_the_mechanism_is_not_offered() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    let out = feed(
        &mut session,
        &[":srv CAP * LS :sasl=SCRAM-SHA-256 server-time"],
    );
    assert_eq!(sent(&out), ["CAP REQ server-time"]);
    let out = feed(&mut session, &[":srv CAP * ACK server-time"]);
    assert_eq!(sent(&out), ["CAP END"]);
}

#[test]
fn ends_negotiation_on_nak_and_without_caps() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    feed(&mut session, &[":srv CAP * LS :server-time"]);
    assert_eq!(
        sent(&feed(&mut session, &[":srv CAP * NAK :server-time"])),
        ["CAP END"]
    );

    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    assert_eq!(sent(&feed(&mut session, &[":srv CAP * LS :"])), ["CAP END"]);
}

#[test]
fn registers_with_servers_without_cap() {
    let mut session = Session::new(
        config(),
        Carry {
            rejoin: vec!["#again".into()],
            ..Carry::default()
        },
    );
    session.start(now());
    let out = feed(&mut session, &[":old.server 001 subspace :Welcome"]);
    assert_eq!(sent(&out), ["JOIN #subspace", "JOIN #again"]);
    assert!(
        out.events.iter().any(
            |event| matches!(event, Event::Capabilities { enabled, .. } if enabled.is_empty())
        )
    );
}

#[test]
fn falls_back_to_alternative_nicks() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    let first = feed(
        &mut session,
        &[":srv 433 * subspace :Nickname is already in use"],
    );
    assert_eq!(sent(&first), ["NICK subspace2"]);
    let second = feed(
        &mut session,
        &[":srv 433 * subspace2 :Nickname is already in use"],
    );
    assert_eq!(sent(&second), ["NICK subspace2_"]);
    let welcome = feed(&mut session, &[":srv 001 subspace2_ :Welcome"]);
    assert!(welcome.events.contains(&Event::OwnNick {
        network: NetworkId("local".into()),
        nick: "subspace2_".into(),
    }));
    let taken = feed(
        &mut session,
        &[":srv 433 subspace2_ other :Nickname is already in use"],
    );
    assert!(taken.send.is_empty());
    assert_eq!(lines(&taken)[0].kind, LineKind::Error);
}

#[test]
fn cap_notify_requests_new_caps_and_reports_removals() {
    let mut session = super::registered("cap-notify");
    let new = feed(
        &mut session,
        &[":srv CAP subspace NEW :away-notify unknown-cap"],
    );
    assert_eq!(sent(&new), ["CAP REQ away-notify"]);
    let ack = feed(&mut session, &[":srv CAP subspace ACK :away-notify"]);
    assert!(ack.events.iter().any(|event| matches!(
        event,
        Event::Capabilities { enabled, .. } if enabled.contains(&"away-notify".to_string())
    )));
    let del = feed(&mut session, &[":srv CAP subspace DEL :away-notify"]);
    assert!(del.events.iter().any(|event| matches!(
        event,
        Event::Capabilities { enabled, .. } if !enabled.contains(&"away-notify".to_string())
    )));
}

#[test]
fn answers_ping_and_shows_errors() {
    let mut session = super::registered("");
    let out = feed(&mut session, &["PING :token here"]);
    assert_eq!(sent(&out), ["PONG :token here"]);
    let out = feed(&mut session, &["ERROR :Closing link"]);
    assert_eq!(lines(&out)[0].text, "Closing link");
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
}
