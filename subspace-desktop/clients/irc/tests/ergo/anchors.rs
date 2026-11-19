//! Resuming after an anchor against Ergo.

use subspace_client_irc::{Anchor, Command, Event, LineKind, Target};

use super::{join, registered_client, server, suffix};

/// A client that holds a channel's lines up to one message, as the IRC
/// board does after restoring its store, asks on joining only for the lines
/// after it, and gets them as one backfill.
#[test]
fn an_anchored_join_asks_only_for_what_came_after() {
    let Some(server) = server() else {
        eprintln!("IRC_TEST_SERVER is not set; skipping the Ergo test");
        return;
    };
    let suffix = suffix();
    let channel = format!("#anchor-{suffix}");
    let here = Target::Channel(channel.clone());
    let mut a = registered_client("a", &format!("ssc{suffix}"), &server);
    let from = join(&mut a, &channel);
    a.wait(from, "A joined", |event| {
        matches!(event, Event::Joined { .. })
    });
    let mut said = Vec::new();
    for text in ["one", "two", "three"] {
        let from = a.mark();
        a.input(here.clone(), text);
        said.push(a.wait_line(from, "echo", |line| line.text == text));
    }
    let mut b = registered_client("b", &format!("ssd{suffix}"), &server);
    b.runtime.send(Command::Anchors {
        network: b.network(),
        anchors: vec![Anchor {
            target: here.clone(),
            id: said[0].id.clone(),
            time: said[0].time,
        }],
    });
    let from = join(&mut b, &channel);
    let backfill = b.wait(from, "the missed lines", |event| {
        matches!(event, Event::Backfill { .. })
    });
    let Event::Backfill { lines, .. } = backfill else {
        unreachable!()
    };
    // Event playback adds our own join, which the board already holds.
    let texts: Vec<&str> = lines
        .iter()
        .filter(|line| line.kind == LineKind::Privmsg)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(texts, ["two", "three"]);
    assert!(
        !b.events[from..]
            .iter()
            .any(|event| matches!(event, Event::History { .. })),
        "no latest page as well"
    );
}
