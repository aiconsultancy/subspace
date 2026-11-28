//! Anchors: a connection asks only for the lines the owner missed, and the
//! lines around one message, answered as `Event::Around`.

use super::{ALL_CAPS, ERGO_ISUPPORT, channel, command, config, feed, in_channel, now, sent};
use crate::model::{Anchor, Command, Event, NetworkId, Target};
use crate::session::{Carry, Session};

fn anchor(target: Target, id: Option<&str>) -> Anchor {
    Anchor {
        target,
        id: id.map(str::to_owned),
        time: now(),
    }
}

/// A session that registers with `anchors` carried in, as after
/// `Command::Anchors` arrived before the connection.
fn resumed(anchors: Vec<Anchor>) -> (Session, Vec<String>) {
    let mut config = config();
    config.sasl = None;
    let carry = Carry {
        anchors,
        ..Carry::default()
    };
    let mut session = Session::new(config, carry);
    session.start(now());
    let caps = ALL_CAPS
        .split_whitespace()
        .map(|cap| cap.split('=').next().unwrap_or(cap))
        .collect::<Vec<_>>()
        .join(" ");
    let out = feed(
        &mut session,
        &[
            &format!(":ergo.test CAP * LS :{ALL_CAPS}"),
            &format!(":ergo.test CAP subspace ACK :{caps}"),
            ":ergo.test 001 subspace :Welcome",
        ],
    );
    let sent = sent(&out);
    (session, sent)
}

fn discovery_session(anchors: Vec<Anchor>, limit: u16) -> (Session, Vec<String>) {
    let mut config = config();
    config.sasl = None;
    let carry = Carry {
        anchors,
        ..Carry::default()
    };
    let mut session = Session::new(config, carry);
    session.start(now());
    let welcome = feed(
        &mut session,
        &[
            ":ergo.test CAP * LS :batch draft/chathistory",
            ":ergo.test CAP subspace ACK :batch draft/chathistory",
            ":ergo.test 900 * * subspace :Logged in",
            ":ergo.test 001 subspace :Welcome",
            &format!(":ergo.test 005 subspace CHATHISTORY={limit} :Supported"),
            ":ergo.test 376 subspace :End of MOTD",
        ],
    );
    (session, sent(&welcome))
}

#[test]
fn a_first_offline_dm_is_discovered_and_delivered_as_replay() {
    let (mut session, welcome) =
        discovery_session(vec![anchor(Target::Query("bob".into()), None)], 1000);
    assert!(welcome.contains(
        &"CHATHISTORY TARGETS timestamp=2026-01-02T03:04:10.000Z timestamp=1970-01-01T00:00:00.000Z 100".into()
    ));
    let discovered = feed(
        &mut session,
        &[
            ":ergo.test BATCH +targets draft/chathistory-targets",
            "@batch=targets :ergo.test CHATHISTORY TARGETS #subspace 2026-01-02T03:04:05.000Z",
            "@batch=targets :ergo.test CHATHISTORY TARGETS bob 2026-01-02T03:04:05.000Z",
            "@batch=targets :ergo.test CHATHISTORY TARGETS subspace 2026-01-02T03:04:05.000Z",
            "@batch=targets :ergo.test CHATHISTORY TARGETS alice 2026-01-02T03:03:05.000Z",
            "@batch=targets :ergo.test CHATHISTORY TARGETS Alice 2026-01-02T03:03:05.000Z",
            ":ergo.test BATCH -targets",
        ],
    );
    assert_eq!(sent(&discovered), ["CHATHISTORY LATEST alice * 500"]);
    assert!(discovered.events.is_empty());
    let reply = feed(
        &mut session,
        &[
            ":ergo.test BATCH +dm chathistory alice",
            "@batch=dm;time=2026-01-02T03:03:05.000Z;msgid=offline :alice!a@h PRIVMSG subspace :hello while away",
            ":ergo.test BATCH -dm",
        ],
    );
    assert!(matches!(
        reply.events.as_slice(),
        [Event::Replayed(line)]
            if line.buffer.target == Target::Query("alice".into()) && line.text == "hello while away"
    ));
    assert!(
        session
            .into_carry()
            .anchors
            .iter()
            .any(|anchor| anchor.id.as_deref() == Some("offline"))
    );
}

#[test]
fn discovery_waits_for_sign_in_when_it_happens_after_welcome() {
    let mut network = config();
    network.sasl = None;
    let mut session = Session::new(network, Carry::default());
    session.start(now());
    let welcome = feed(
        &mut session,
        &[
            ":ergo.test CAP * LS :batch draft/chathistory",
            ":ergo.test CAP subspace ACK :batch draft/chathistory",
            ":ergo.test 001 subspace :Welcome",
            ERGO_ISUPPORT,
        ],
    );
    assert_eq!(
        sent(&welcome).last().map(String::as_str),
        Some("JOIN #subspace")
    );

    let login = ":ergo.test 900 subspace subspace!u@h subspace :Logged in";
    assert_eq!(
        sent(&feed(&mut session, &[login])),
        [
            "CHATHISTORY TARGETS timestamp=2026-01-02T03:04:10.000Z timestamp=1970-01-01T00:00:00.000Z 100"
        ]
    );
    assert!(feed(&mut session, &[login]).send.is_empty());
}

#[test]
fn discovery_pages_past_known_conversations_and_respects_the_server_limit() {
    let (mut session, welcome) = discovery_session(vec![], 2);
    assert!(welcome.iter().any(|line| line.ends_with("00.000Z 2")));
    let first = feed(
        &mut session,
        &[
            ":ergo.test BATCH +a draft/chathistory-targets",
            "@batch=a :ergo.test CHATHISTORY TARGETS #one 2026-01-02T03:04:04.000Z",
            "@batch=a :ergo.test CHATHISTORY TARGETS #two 2026-01-02T03:04:03.000Z",
            ":ergo.test BATCH -a",
        ],
    );
    assert_eq!(
        sent(&first),
        [
            "CHATHISTORY TARGETS timestamp=2026-01-02T03:04:03.000Z timestamp=1970-01-01T00:00:00.000Z 2"
        ]
    );
    let second = feed(
        &mut session,
        &[
            "@draft/chathistory-end :ergo.test BATCH +b draft/chathistory-targets",
            "@batch=b :ergo.test CHATHISTORY TARGETS alice 2026-01-02T03:04:02.000Z",
            ":ergo.test BATCH -b",
        ],
    );
    assert_eq!(sent(&second), ["CHATHISTORY LATEST alice * 2"]);
}

#[test]
fn failed_target_discovery_leaves_known_dm_catch_up_pending() {
    let (mut session, _) = discovery_session(vec![anchor(Target::Query("bob".into()), None)], 1000);
    let failure = feed(
        &mut session,
        &["@label=subspace-history-targets :ergo.test FAIL CHATHISTORY MESSAGE_ERROR :No context"],
    );
    assert!(backfill(&failure.events).is_none());
    let reply = feed(
        &mut session,
        &[
            ":ergo.test BATCH +dm chathistory bob",
            "@batch=dm;time=2026-01-02T03:04:05.000Z :bob!b@h PRIVMSG subspace :still missed",
            ":ergo.test BATCH -dm",
        ],
    );
    assert_eq!(backfill(&reply.events).unwrap().1, ["still missed"]);
}

fn backfill(events: &[Event]) -> Option<(&Target, Vec<&str>)> {
    events.iter().find_map(|event| match event {
        Event::Backfill { buffer, lines } | Event::Around { buffer, lines } => Some((
            &buffer.target,
            lines.iter().map(|line| line.text.as_str()).collect(),
        )),
        _ => None,
    })
}

#[test]
fn joining_an_anchored_channel_asks_for_what_came_after_and_merges_it() {
    let (mut session, _) = resumed(vec![anchor(Target::Channel("#C".into()), Some("m9"))]);
    let join = feed(&mut session, &[":subspace!u@h JOIN #c * :Subspace"]);
    assert_eq!(sent(&join), ["CHATHISTORY LATEST #c msgid=m9 500"]);
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +b chathistory #c",
            "@batch=b;time=2026-01-02T03:05:00.000Z;msgid=m10 :alice!a@h PRIVMSG #c :missed",
            ":ergo.test BATCH -b",
        ],
    );
    let (target, texts) = backfill(&out.events).unwrap();
    assert_eq!(*target, Target::Channel("#c".into()));
    assert_eq!(texts, ["missed"]);
    assert!(
        !out.events
            .iter()
            .any(|event| matches!(event, Event::History { .. }))
    );
}

#[test]
fn an_anchor_without_a_msgid_uses_its_time_and_the_newest_line_wins() {
    let (mut session, _) = resumed(vec![anchor(Target::Channel("#c".into()), None)]);
    feed(
        &mut session,
        &["@time=2026-01-02T04:00:00.000Z;msgid=new :alice!a@h PRIVMSG #c :later"],
    );
    let carried = session.into_carry();
    assert_eq!(carried.anchors.len(), 1);
    assert_eq!(carried.anchors[0].id.as_deref(), Some("new"));
    let (_, sent) = resumed(vec![anchor(Target::Query("bob".into()), None)]);
    assert!(
        sent.contains(&"CHATHISTORY LATEST bob timestamp=2026-01-02T03:04:05.000Z 500".to_owned()),
        "{sent:?}"
    );
}

#[test]
fn anchors_sent_as_a_command_are_kept_for_the_next_join() {
    let mut carry = Carry::default();
    carry.forget(&Command::Anchors {
        network: NetworkId("local".into()),
        anchors: vec![anchor(Target::Channel("#c".into()), Some("a1"))],
    });
    assert_eq!(carry.anchors.len(), 1);
    let mut session = in_channel(ALL_CAPS);
    command(
        &mut session,
        Command::Anchors {
            network: NetworkId("local".into()),
            anchors: vec![anchor(Target::Channel("#d".into()), Some("d1"))],
        },
    );
    let join = feed(&mut session, &[":subspace!u@h JOIN #d * :Subspace"]);
    assert_eq!(sent(&join), ["CHATHISTORY LATEST #d msgid=d1 500"]);
}

#[test]
fn load_around_asks_by_msgid_and_requests_answer_in_order() {
    let mut session = in_channel(ALL_CAPS);
    let out = command(
        &mut session,
        Command::LoadAround {
            buffer: channel("#c"),
            id: Some("x1".into()),
            time: now(),
            limit: 50,
        },
    );
    assert_eq!(sent(&out), ["CHATHISTORY AROUND #c msgid=x1 50"]);
    // The join's LATEST is answered first, then the AROUND.
    let first = feed(
        &mut session,
        &[":ergo.test BATCH +j chathistory #c", ":ergo.test BATCH -j"],
    );
    assert!(backfill(&first.events).is_none());
    let second = feed(
        &mut session,
        &[
            ":ergo.test BATCH +a chathistory #c",
            "@batch=a;time=2025-06-01T00:00:00.000Z;msgid=x1 :bob!b@h PRIVMSG #c :found",
            ":ergo.test BATCH -a",
        ],
    );
    assert_eq!(backfill(&second.events).unwrap().1, ["found"]);
}

#[test]
fn load_around_without_chathistory_answers_empty() {
    let mut session = in_channel("batch message-tags server-time");
    let out = command(
        &mut session,
        Command::LoadAround {
            buffer: channel("#c"),
            id: None,
            time: now(),
            limit: 50,
        },
    );
    assert!(sent(&out).is_empty());
    assert_eq!(backfill(&out.events).map(|(_, texts)| texts.len()), Some(0));
}
