//! Signing in with a login a credential source issued: the password goes
//! out in one PLAIN exchange and is dropped, the outcome is told in fixed
//! words, and a server that registers without accepting it is left before
//! anything is joined.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::super::{Carry, Ending, Session, sign_in_with};
use super::{config, feed, lines, now, sent};
use crate::model::{ConnectionStatus, Event, IssuedPassword, LineKind, Sasl};

/// A session that will sign in as `ada@desk1` with an issued password.
fn signing_in() -> Session {
    let mut config = config();
    config.sasl = Some(Sasl {
        username: "ada@desk1".into(),
        password: String::new(),
        password_file: None,
        external: false,
        credential: None,
    });
    let session = Session::new(config, Carry::default());
    sign_in_with(session, IssuedPassword::new("issued-pw"))
}

/// [`signing_in`], past capability negotiation with SASL offered.
fn issued() -> Session {
    let mut session = signing_in();
    session.start(now());
    feed(
        &mut session,
        &[":srv CAP * LS :sasl", ":srv CAP * ACK :sasl"],
    );
    session
}

fn connected(out: &crate::session::Output) -> bool {
    out.events.iter().any(|event| {
        matches!(
            event,
            Event::Status {
                status: ConnectionStatus::Connected { .. },
                ..
            }
        )
    })
}

#[test]
fn plain_sends_a_device_username_as_both_identities() {
    // Ergo checks that both identities match before it takes the device
    // off the account name, so `account@device` goes in both.
    let mut config = config();
    config.sasl = Some(Sasl {
        username: "ada@desk1".into(),
        password: "pw".into(),
        password_file: None,
        external: false,
        credential: None,
    });
    let mut session = Session::new(config, Carry::default());
    session.start(now());
    feed(
        &mut session,
        &[":srv CAP * LS :sasl", ":srv CAP * ACK :sasl"],
    );
    let out = feed(&mut session, &["AUTHENTICATE +"]);
    let payload = STANDARD.encode("ada@desk1\0ada@desk1\0pw");
    assert_eq!(sent(&out), [format!("AUTHENTICATE {payload}")]);
}

#[test]
fn an_issued_password_goes_out_once_then_is_dropped() {
    let mut session = issued();
    let first = feed(&mut session, &["AUTHENTICATE +"]);
    let payload = STANDARD.encode("ada@desk1\0ada@desk1\0issued-pw");
    assert_eq!(sent(&first), [format!("AUTHENTICATE {payload}")]);
    let again = feed(&mut session, &["AUTHENTICATE +"]);
    let empty = STANDARD.encode("ada@desk1\0ada@desk1\0");
    assert_eq!(
        sent(&again),
        [format!("AUTHENTICATE {empty}")],
        "the session no longer holds it"
    );
}

#[test]
fn an_accepted_sign_in_registers_and_joins() {
    let mut session = issued();
    feed(&mut session, &["AUTHENTICATE +"]);
    let accepted = feed(&mut session, &[":srv 903 ada :echo issued-pw"]);
    assert_eq!(sent(&accepted), ["CAP END"]);
    assert_eq!(
        lines(&accepted)[0].text,
        "The server accepted the sign-in (903)"
    );
    let welcome = feed(&mut session, &[":srv 001 ada :Welcome"]);
    assert!(connected(&welcome));
    assert!(welcome.ending.is_none());
    assert!(sent(&welcome).iter().any(|line| line == "JOIN #subspace"));
}

#[test]
fn a_refused_sign_in_is_told_in_fixed_words_and_left_at_once() {
    let mut session = issued();
    feed(&mut session, &["AUTHENTICATE +"]);
    let failed = feed(&mut session, &[":srv 904 ada :bad password issued-pw"]);
    let line = lines(&failed)[0];
    assert_eq!(line.kind, LineKind::Error);
    assert_eq!(line.text, "The server did not accept the sign-in (904)");
    assert!(
        matches!(failed.ending, Some(Ending::Refused(_))),
        "the network drops the login and retries"
    );
    assert_eq!(sent(&failed), ["QUIT :Sign-in required"], "no registration");
    assert!(!connected(&failed));
}

#[test]
fn a_login_that_may_no_longer_be_sent_is_not() {
    let mut config = config();
    config.sasl = Some(Sasl {
        username: "ada@desk1".into(),
        password: String::new(),
        password_file: None,
        external: false,
        credential: None,
    });
    let stale = IssuedPassword::new("issued-pw").while_valid(|| false);
    let mut session = sign_in_with(Session::new(config, Carry::default()), stale);
    session.start(now());
    feed(
        &mut session,
        &[":srv CAP * LS :sasl", ":srv CAP * ACK :sasl"],
    );
    let out = feed(&mut session, &["AUTHENTICATE +"]);
    assert_eq!(sent(&out), ["QUIT :Sign-in required"]);
    assert!(matches!(out.ending, Some(Ending::Refused(_))));
}

#[test]
fn a_server_without_sasl_cannot_register_an_issued_sign_in() {
    let mut session = signing_in();
    session.start(now());
    let ls = feed(&mut session, &[":srv CAP * LS :server-time"]);
    assert!(!sent(&ls).iter().any(|line| line.contains("issued-pw")));
    let welcome = feed(&mut session, &[":srv 001 ada :Welcome"]);
    assert!(matches!(welcome.ending, Some(Ending::Refused(_))));
    assert!(!connected(&welcome));
}
