//! CTCP replies and rate limits.

use chrono::TimeDelta;

use super::super::{PLAIN_CAPS, feed, lines, now, registered, sent};
use crate::proto::parse;

#[test]
fn answers_ctcp_version_ping_and_time() {
    let mut session = registered(PLAIN_CAPS);
    for (elapsed, query, expected) in [
        (
            0,
            "VERSION",
            concat!("VERSION Subspace IRC ", env!("CARGO_PKG_VERSION")),
        ),
        (3, "PING 123", "PING 123"),
        (6, "TIME", "TIME Fri, 2 Jan 2026 03:04:11 +0000"),
    ] {
        let message = parse(&format!(":bob!b@h PRIVMSG subspace :\u{1}{query}\u{1}")).unwrap();
        let out = session.receive(message, now() + TimeDelta::seconds(elapsed));
        assert_eq!(sent(&out), [format!("NOTICE bob :\u{1}{expected}\u{1}")]);
        assert!(lines(&out).is_empty());
    }
}

#[test]
fn ctcp_cooldown_is_shared_by_equivalent_nicks_and_query_types() {
    let mut session = registered(PLAIN_CAPS);
    feed(
        &mut session,
        &[":srv 005 subspace CASEMAPPING=rfc1459 :are supported"],
    );
    let version = parse(":[Lee]!u@h PRIVMSG #c :\u{1}VERSION\u{1}").unwrap();
    let ping = parse(":{lee}!u@h PRIVMSG subspace :\u{1}PING 123\u{1}").unwrap();
    assert_eq!(session.receive(version, now()).send.len(), 1);
    assert!(
        session
            .receive(ping.clone(), now() + TimeDelta::seconds(2))
            .send
            .is_empty()
    );
    assert_eq!(
        sent(&session.receive(ping, now() + TimeDelta::seconds(3))),
        ["NOTICE {lee} :\u{1}PING 123\u{1}"]
    );
}

#[test]
fn ctcp_budget_is_shared_across_senders_and_recovers_as_replies_age() {
    let mut session = registered(PLAIN_CAPS);
    for sender in 0..10 {
        let message = parse(&format!(
            ":nick{sender}!u@h PRIVMSG subspace :\u{1}PING\u{1}"
        ))
        .unwrap();
        assert_eq!(
            session
                .receive(message, now() + TimeDelta::seconds(sender))
                .send
                .len(),
            1
        );
    }
    let query = parse(":another!u@h PRIVMSG subspace :\u{1}VERSION\u{1}").unwrap();
    assert!(
        session
            .receive(query.clone(), now() + TimeDelta::seconds(59))
            .send
            .is_empty()
    );
    assert_eq!(
        session
            .receive(query, now() + TimeDelta::seconds(60))
            .send
            .len(),
        1
    );
    let query = parse(":extra!u@h PRIVMSG subspace :\u{1}TIME\u{1}").unwrap();
    assert!(
        session
            .receive(query, now() + TimeDelta::seconds(60))
            .send
            .is_empty()
    );
}
