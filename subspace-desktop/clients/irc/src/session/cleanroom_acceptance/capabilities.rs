//! Capability and SASL acceptance tests.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::{Carry, Session, at, base, capabilities, feed, lines, welcomed, wire};
use crate::model::{LineKind, NetworkConfig, Sasl};

fn with_sasl(username: &str, password: &str, external: bool) -> NetworkConfig {
    NetworkConfig {
        sasl: Some(Sasl {
            username: username.into(),
            password: password.into(),
            password_file: None,
            external,
            credential: None,
        }),
        ..base()
    }
}

#[test]
fn k01_a_multi_line_ls_is_answered_once_in_canonical_order() {
    let mut session = Session::new(with_sasl("kim", "pw", false), Carry::default());
    session.start(at());
    assert!(
        feed(
            &mut session,
            &[":s CAP * LS * :server-time sasl=PLAIN unknown/x"]
        )
        .send
        .is_empty()
    );
    assert!(
        feed(&mut session, &[":s CAP * LS * :away-notify message-tags"])
            .send
            .is_empty()
    );
    let out = feed(&mut session, &[":s CAP * LS :batch account-tag"]);
    assert_eq!(
        wire(&out),
        ["CAP REQ :account-tag away-notify batch message-tags server-time sasl"]
    );
}

#[test]
fn k02_sasl_is_requested_only_with_credentials_and_an_offered_mechanism() {
    let cases: [(NetworkConfig, &str, &str); 5] = [
        (base(), "sasl server-time", "CAP REQ server-time"),
        (with_sasl("kim", "pw", false), "sasl", "CAP REQ sasl"),
        (with_sasl("kim", "pw", false), "sasl=plain", "CAP REQ sasl"),
        (
            with_sasl("", "", true),
            "sasl=PLAIN server-time",
            "CAP REQ server-time",
        ),
        (
            with_sasl("", "", true),
            "sasl=EXTERNAL,PLAIN",
            "CAP REQ sasl",
        ),
    ];
    for (config, offered, expected) in cases {
        let mut session = Session::new(config, Carry::default());
        session.start(at());
        let out = feed(&mut session, &[&format!(":s CAP * LS :{offered}")]);
        assert_eq!(wire(&out), [expected], "{offered}");
    }
}

#[test]
fn k03_metadata_needs_batch_on_offer() {
    for (offered, expected) in [
        ("draft/metadata-2", "CAP END"),
        ("draft/metadata-2 batch", "CAP REQ :batch draft/metadata-2"),
    ] {
        let mut session = Session::new(base(), Carry::default());
        session.start(at());
        assert_eq!(
            wire(&feed(&mut session, &[&format!(":s CAP * LS :{offered}")])),
            [expected]
        );
    }
}

#[test]
fn k04_negotiation_ends_after_the_final_ack_and_reports_once() {
    let mut session = Session::new(base(), Carry::default());
    session.start(at());
    feed(
        &mut session,
        &[":s CAP * LS :batch echo-message server-time"],
    );
    let partial = feed(&mut session, &[":s CAP kim ACK * :batch echo-message"]);
    assert!(partial.send.is_empty());
    assert!(capabilities(&partial).is_empty());
    let done = feed(&mut session, &[":s CAP kim ACK :server-time"]);
    assert_eq!(wire(&done), ["CAP END"]);
    assert_eq!(
        capabilities(&done),
        [vec![
            "batch".to_string(),
            "echo-message".into(),
            "server-time".into()
        ]]
    );
}

#[test]
fn k05_a_nak_ends_negotiation_with_nothing_enabled() {
    let mut session = Session::new(base(), Carry::default());
    session.start(at());
    feed(&mut session, &[":s CAP * LS :batch server-time"]);
    let out = feed(&mut session, &[":s CAP kim NAK :batch server-time"]);
    assert_eq!(wire(&out), ["CAP END"]);
    assert_eq!(capabilities(&out), [Vec::<String>::new()]);
}

#[test]
fn k06_plain_payloads_are_base64_in_400_byte_chunks() {
    let payload = |username: &str, password: &str| {
        let mut session = Session::new(with_sasl(username, password, false), Carry::default());
        session.start(at());
        feed(&mut session, &[":s CAP * LS :sasl", ":s CAP kim ACK :sasl"]);
        wire(&feed(&mut session, &["AUTHENTICATE +"]))
    };
    let short = payload("kim", "hunter2");
    assert_eq!(
        short,
        [format!(
            "AUTHENTICATE {}",
            STANDARD.encode("kim\0kim\0hunter2")
        )]
    );
    // 2 × 100 + 398 + 2 separators = 600 bytes, 800 in base64.
    let exact = payload(&"a".repeat(100), &"b".repeat(398));
    assert_eq!(exact.len(), 3);
    assert_eq!(exact[0].len(), "AUTHENTICATE ".len() + 400);
    assert_eq!(exact[1].len(), "AUTHENTICATE ".len() + 400);
    assert_eq!(exact[2], "AUTHENTICATE +");
    // 601 bytes: 804 in base64, so a short last chunk and no "+".
    let over = payload(&"a".repeat(100), &"b".repeat(399));
    assert_eq!(over.len(), 3);
    assert_eq!(over[2].len(), "AUTHENTICATE ".len() + 4);
    assert_eq!(payload("", ""), ["AUTHENTICATE AAA="]);
}

#[test]
fn k07_external_answers_plus_and_other_challenges_are_ignored() {
    let mut session = Session::new(with_sasl("", "", true), Carry::default());
    session.start(at());
    feed(
        &mut session,
        &[":s CAP * LS :sasl=EXTERNAL", ":s CAP kim ACK :sasl"],
    );
    assert!(feed(&mut session, &["AUTHENTICATE abc"]).send.is_empty());
    assert_eq!(
        wire(&feed(&mut session, &["AUTHENTICATE +"])),
        ["AUTHENTICATE +"]
    );
}

#[test]
fn k08_sasl_outcomes_end_negotiation() {
    for (numeric, kind) in [
        ("903", LineKind::Info),
        ("907", LineKind::Info),
        ("902", LineKind::Error),
        ("904", LineKind::Error),
        ("905", LineKind::Error),
        ("906", LineKind::Error),
    ] {
        let mut session = Session::new(with_sasl("kim", "pw", false), Carry::default());
        session.start(at());
        feed(&mut session, &[":s CAP * LS :sasl", ":s CAP kim ACK :sasl"]);
        let waiting = feed(&mut session, &["AUTHENTICATE +"]);
        assert!(!wire(&waiting).contains(&"CAP END".to_string()));
        let out = feed(&mut session, &[&format!(":s {numeric} kim :outcome text")]);
        assert_eq!(wire(&out), ["CAP END"], "{numeric}");
        assert_eq!(lines(&out)[0].kind, kind, "{numeric}");
        assert_eq!(lines(&out)[0].text, "outcome text");
    }
}

#[test]
fn k09_cap_notify_new_and_del_after_registration() {
    let mut session = welcomed(with_sasl("kim", "pw", false), "cap-notify");
    let new = feed(&mut session, &[":s CAP kim NEW :sasl invite-notify"]);
    assert_eq!(
        wire(&new),
        ["CAP REQ invite-notify"],
        "no SASL after registration"
    );
    let ack = feed(&mut session, &[":s CAP kim ACK :invite-notify"]);
    assert!(ack.send.is_empty());
    assert_eq!(
        capabilities(&ack),
        [vec!["cap-notify".to_string(), "invite-notify".into()]]
    );
    let del = feed(&mut session, &[":s CAP kim DEL :invite-notify"]);
    assert_eq!(capabilities(&del), [vec!["cap-notify".to_string()]]);
    let again = feed(&mut session, &[":s CAP kim NEW :invite-notify"]);
    assert_eq!(wire(&again), ["CAP REQ invite-notify"]);
    assert!(
        feed(&mut session, &[":s CAP kim LIST :cap-notify"])
            .events
            .is_empty()
    );
}
