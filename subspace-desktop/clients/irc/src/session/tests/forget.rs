//! `Command::Forget`: a channel the user removed from the list is skipped at
//! connect, and a private conversation they closed stops taking notices.

use super::super::{Carry, Session};
use super::{PLAIN_CAPS, buffer, config, feed, lines, now, registered, sent};
use crate::model::{Command, NetworkId, Target};

/// `Command::Forget` of `target` on the fixture network.
fn forget(target: Target) -> Command {
    Command::Forget {
        buffer: buffer(target),
    }
}

#[test]
fn a_removed_channel_is_not_joined_after_a_reconnect_but_a_parted_one_is() {
    let removed = forget(Target::Channel("#subspace".into()));
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    feed(
        &mut session,
        &[
            ":old.server 001 subspace :Welcome",
            ":subspace!u@h JOIN #subspace",
            ":subspace!u@h PART #subspace :bye",
        ],
    );
    assert!(session.left.is_empty(), "a typed /part is not removal");
    let mut parted = Session::new(config(), session.into_carry());
    parted.start(now());
    let welcome = feed(&mut parted, &[":old.server 001 subspace :Welcome"]);
    assert_eq!(
        sent(&welcome).last().map(String::as_str),
        Some("JOIN #subspace")
    );

    // Removed while still joined, before the PART echo: the connection drops
    // with the channel in both the configured and the rejoin lists.
    feed(&mut parted, &[":subspace!u@h JOIN #subspace"]);
    parted.command(removed, now());
    let carry = parted.into_carry();
    assert_eq!(carry.left, ["#subspace"]);
    assert_eq!(carry.rejoin, ["#subspace"]);
    let mut next = Session::new(config(), carry);
    next.start(now());
    let welcome = feed(&mut next, &[":old.server 001 subspace :Welcome"]);
    assert!(
        sent(&welcome).iter().all(|line| !line.starts_with("JOIN")),
        "{:?}",
        sent(&welcome)
    );

    feed(&mut next, &[":subspace!u@h JOIN #Subspace"]);
    assert!(next.into_carry().left.is_empty(), "joining again clears it");
}

#[test]
fn a_channel_removed_while_not_joined_is_skipped_too() {
    let removed = forget(Target::Channel("#Subspace".into()));
    // While connected, as after a kick.
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    feed(&mut session, &[":old.server 001 subspace :Welcome"]);
    assert!(session.command(removed.clone(), now()).send.is_empty());
    assert_eq!(session.into_carry().left, ["#Subspace"]);

    // While disconnected, the network task keeps it in the carry.
    let mut carry = Carry::default();
    carry.forget(&removed);
    carry.forget(&removed);
    carry.forget(&Command::Connect(NetworkId("local".into())));
    assert_eq!(carry.left, ["#Subspace"], "once, and only for this command");
    let mut next = Session::new(config(), carry);
    next.start(now());
    let welcome = feed(&mut next, &[":old.server 001 subspace :Welcome"]);
    assert!(
        sent(&welcome).iter().all(|line| !line.starts_with("JOIN")),
        "ASCII case is ignored"
    );
}

#[test]
fn removal_matches_ascii_case_but_not_rfc1459_brackets() {
    let mut config = config();
    config.channels = vec!["#a{1}".into(), "#b".into()];
    let mut carry = Carry::default();
    carry.forget(&forget(Target::Channel("#A[1]".into())));
    let mut session = Session::new(config, carry);
    session.start(now());
    let welcome = feed(&mut session, &[":old.server 001 subspace :Welcome"]);
    let joins: Vec<String> = sent(&welcome)
        .into_iter()
        .filter(|line| line.starts_with("JOIN"))
        .collect();
    assert_eq!(
        joins,
        ["JOIN #a{1}", "JOIN #b"],
        "#a{{1}} is another channel"
    );
}

#[test]
fn a_forgotten_query_sends_notices_to_the_server_again() {
    let mut session = registered(PLAIN_CAPS);
    feed(&mut session, &[":NickServ!s@services PRIVMSG subspace :hi"]);
    let notice = ":NickServ!s@services NOTICE subspace :You are now identified";
    assert_eq!(
        lines(&feed(&mut session, &[notice]))[0].buffer.target,
        Target::Query("NickServ".into()),
        "the open query takes the notice"
    );
    let closed = session.command(forget(Target::Query("nickserv".into())), now());
    assert!(closed.send.is_empty() && closed.events.is_empty());
    assert_eq!(
        lines(&feed(&mut session, &[notice]))[0].buffer.target,
        Target::Server,
        "closed: the notice goes where it went before the query opened"
    );
}
