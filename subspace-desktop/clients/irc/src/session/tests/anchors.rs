//! Anchors: a connection asks only for the lines the owner missed, and the
//! lines around one message, answered as `Event::Backfill`.

use super::{ALL_CAPS, channel, command, config, feed, in_channel, now, sent};
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

fn backfill(events: &[Event]) -> Option<(&Target, Vec<&str>)> {
    events.iter().find_map(|event| match event {
        Event::Backfill { buffer, lines } => Some((
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
