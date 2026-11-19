use super::{ALL_CAPS, buffer, channel, command, feed, in_channel, lines, now, sent};
use crate::model::{Command, Event, LineKind, Target};

fn history(events: &[Event]) -> Option<(&Vec<crate::model::Line>, bool)> {
    events.iter().find_map(|event| match event {
        Event::History {
            lines, complete, ..
        } => Some((lines, *complete)),
        _ => None,
    })
}

#[test]
fn chathistory_batches_become_one_history_event() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +h1 chathistory #c",
            "@batch=h1;time=2024-01-01T00:00:00.000Z;msgid=a :alice!a@h PRIVMSG #c :oldest",
            "@batch=h1;time=2024-01-01T00:00:01.000Z;msgid=b :bob!b@h JOIN #c",
            "@batch=h1;time=2024-01-01T00:00:02.000Z;msgid=c :subspace!u@h PRIVMSG #c :mine",
            "@batch=h1 :ergo.test BATCH +ml draft/multiline #c",
            "@batch=ml;msgid=d :alice!a@h PRIVMSG #c :two",
            "@batch=ml :alice!a@h PRIVMSG #c :lines",
            ":ergo.test BATCH -ml",
            ":ergo.test BATCH -h1",
        ],
    );
    let (history, complete) = history(&out.events).unwrap();
    // Fewer than the 100 requested on join: nothing older exists.
    assert!(complete);
    let texts: Vec<&str> = history.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(texts, ["oldest", "joined", "mine", "two\nlines"]);
    assert_eq!(history[1].kind, LineKind::Join);
    assert!(history[2].own);
    assert!(history.iter().all(|line| line.buffer == channel("#c")));
    assert!(lines(&out).is_empty());
    // Playback does not change the member list.
    assert!(
        !out.events
            .iter()
            .any(|event| matches!(event, Event::MemberJoined { .. }))
    );
}

#[test]
fn load_history_requests_older_pages() {
    let mut session = in_channel(ALL_CAPS);
    // The server answers the join's request first, in order.
    feed(
        &mut session,
        &[":ergo.test BATCH +j chathistory #c", ":ergo.test BATCH -j"],
    );
    let out = command(
        &mut session,
        Command::LoadHistory {
            buffer: channel("#c"),
            before: Some(now()),
            limit: 2,
        },
    );
    assert_eq!(
        sent(&out),
        ["CHATHISTORY BEFORE #c timestamp=2026-01-02T03:04:05.000Z 2"]
    );
    let page = feed(
        &mut session,
        &[
            "@batch=x :ergo.test BATCH +p chathistory #c",
            "@batch=p :a!a@h PRIVMSG #c :1",
            "@batch=p :a!a@h PRIVMSG #c :2",
            ":ergo.test BATCH -p",
        ],
    );
    assert_eq!(
        history(&page.events).map(|(_, complete)| complete),
        Some(false)
    );
}

#[test]
fn query_history_goes_to_the_query_buffer() {
    let mut session = in_channel(ALL_CAPS);
    let open = command(
        &mut session,
        Command::Query {
            network: crate::model::NetworkId("local".into()),
            nick: "bob".into(),
        },
    );
    assert_eq!(sent(&open), ["CHATHISTORY LATEST bob * 100"]);
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +q chathistory bob",
            "@batch=q :bob!b@h PRIVMSG subspace :hello",
            ":ergo.test BATCH -q",
            ":bob!b@h NOTICE subspace :now in the query",
        ],
    );
    let (history, _) = history(&out.events).unwrap();
    assert_eq!(history[0].buffer, buffer(Target::Query("bob".into())));
    assert_eq!(lines(&out)[0].buffer, buffer(Target::Query("bob".into())));
}

/// Servers replay missed messages on join or reconnect in `chathistory`
/// batches, unasked, with `draft/event-playback` events among them. The
/// board notifies only for live lines, so none of these may arrive as one.
#[test]
fn replayed_tags_and_event_playback_stay_history() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +r chathistory #c",
            "@batch=r;time=2024-01-01T00:00:00.000Z;msgid=r1 :alice!a@h PRIVMSG #c :@subspace ping",
            "@batch=r;time=2024-01-01T00:00:01.000Z;msgid=r2 :bob!b@h PART #c :later",
            ":ergo.test BATCH -r",
            ":ergo.test BATCH +d chathistory carol",
            "@batch=d;time=2024-01-01T00:00:02.000Z;msgid=r3 :carol!c@h PRIVMSG subspace :are you there?",
            ":ergo.test BATCH -d",
        ],
    );
    assert!(lines(&out).is_empty(), "nothing replayed is a live line");
    let pages: Vec<&Vec<crate::model::Line>> = out
        .events
        .iter()
        .filter_map(|event| match event {
            Event::History { lines, .. } => Some(lines),
            _ => None,
        })
        .collect();
    assert_eq!(pages.len(), 2);
    assert!(pages[0][0].highlight, "the tag still reads as a mention");
    assert_eq!(pages[0][1].kind, LineKind::Part);
    assert_eq!(pages[1][0].buffer, buffer(Target::Query("carol".into())));
}

/// ZNC replays its buffers on connect in `znc.in/playback` batches, not
/// `chathistory`: the lines arrive in order, marked as replayed.
#[test]
fn znc_playback_lines_arrive_marked_as_replayed() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":irc.znc.in BATCH +z znc.in/playback #c",
            "@batch=z;time=2024-01-01T00:00:00.000Z :alice!a@h PRIVMSG #c :@subspace while you were away",
            ":irc.znc.in BATCH -z",
            ":bob!b@h PRIVMSG #c :live again",
        ],
    );
    let replayed: Vec<&crate::model::Line> = out
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Replayed(line) => Some(line),
            _ => None,
        })
        .collect();
    assert_eq!(replayed.len(), 1);
    assert!(replayed[0].highlight, "still a mention, for the badges");
    let live: Vec<&str> = lines(&out).iter().map(|line| line.text.as_str()).collect();
    assert_eq!(live, ["live again"]);
}

#[test]
fn other_batches_pass_through() {
    let mut session = in_channel(ALL_CAPS);
    let out = feed(
        &mut session,
        &[
            ":ergo.test BATCH +n netsplit a.example b.example",
            "@batch=n :alice!a@h QUIT :a.example b.example",
            ":ergo.test BATCH -n",
        ],
    );
    assert_eq!(lines(&out)[0].kind, LineKind::Quit);
}

fn ended(events: &[Event]) -> Vec<&crate::model::BufferKey> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::History {
                buffer,
                lines,
                complete: false,
            } if lines.is_empty() => Some(buffer),
            _ => None,
        })
        .collect()
}

#[test]
fn a_failed_chathistory_ends_the_request_it_names() {
    let mut session = in_channel(ALL_CAPS);
    command(
        &mut session,
        Command::Query {
            network: crate::model::NetworkId("local".into()),
            nick: "Bob".into(),
        },
    );
    let out = feed(
        &mut session,
        &[":ergo.test FAIL CHATHISTORY INVALID_TARGET LATEST bob :Messages could not be retrieved"],
    );
    assert_eq!(ended(&out.events), [&buffer(Target::Query("Bob".into()))]);
    assert_eq!(lines(&out)[0].kind, LineKind::Error);
    let out = feed(
        &mut session,
        &[":ergo.test FAIL CHATHISTORY MESSAGE_ERROR LATEST :Messages could not be retrieved"],
    );
    assert_eq!(ended(&out.events), [&channel("#c")]);
    assert!(session.abandon_history().is_empty());
}

#[test]
fn an_ended_connection_answers_pending_history_requests() {
    let mut session = in_channel(ALL_CAPS);
    let events = session.abandon_history();
    assert_eq!(ended(&events), [&channel("#c")]);
    assert_eq!(events.len(), 1);
    assert!(session.abandon_history().is_empty());
}
