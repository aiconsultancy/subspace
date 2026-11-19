//! `LIST`: asked once after the message of the day and on request, one at a
//! time, its replies collected into one event.

use super::{ALL_CAPS, Carry, Session, command, config, feed, lines, now, registered, sent};
use crate::model::{Command, Event, ListedChannel, NetworkId};

fn list_channels() -> Command {
    Command::ListChannels {
        network: NetworkId("local".into()),
    }
}

fn listed(name: &str, users: u32, topic: &str) -> ListedChannel {
    ListedChannel {
        name: name.into(),
        users,
        topic: topic.into(),
    }
}

fn channel_lists(events: &[Event]) -> Vec<&Vec<ListedChannel>> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::ChannelList { channels, .. } => Some(channels),
            _ => None,
        })
        .collect()
}

#[test]
fn the_end_of_the_motd_asks_for_the_list_once_per_connection() {
    let mut session = registered(ALL_CAPS);
    let first = feed(&mut session, &[":ergo.test 376 subspace :End of MOTD"]);
    assert_eq!(sent(&first), ["LIST"]);
    assert_eq!(lines(&first).len(), 1, "the MOTD's end still shows");
    feed(&mut session, &[":ergo.test 323 subspace :End of LIST"]);
    let again = feed(
        &mut session,
        &[":ergo.test 422 subspace :MOTD File is missing"],
    );
    assert!(sent(&again).is_empty(), "a later MOTD does not ask again");

    let mut next = registered(ALL_CAPS);
    let reconnected = feed(
        &mut next,
        &[":ergo.test 422 subspace :MOTD File is missing"],
    );
    assert_eq!(sent(&reconnected), ["LIST"], "a new connection asks again");
}

#[test]
fn replies_collect_into_one_event_without_server_lines() {
    let mut session = registered(ALL_CAPS);
    feed(&mut session, &[":ergo.test 376 subspace :End of MOTD"]);
    let out = feed(
        &mut session,
        &[
            ":ergo.test 321 subspace Channel :Users Name",
            ":ergo.test 322 subspace #rust 12 :[+nt] Systems programming",
            ":ergo.test 322 subspace #quiet 1 :",
            ":ergo.test 322 subspace * 3 :",
            ":ergo.test 323 subspace :End of LIST",
        ],
    );
    assert!(lines(&out).is_empty(), "{:?}", lines(&out));
    assert_eq!(
        out.events,
        [Event::ChannelList {
            network: NetworkId("local".into()),
            channels: vec![
                listed("#rust", 12, "[+nt] Systems programming"),
                listed("#quiet", 1, ""),
            ],
        }]
    );
}

#[test]
fn one_list_runs_at_a_time_and_a_refusal_ends_it() {
    let mut session = registered(ALL_CAPS);
    assert_eq!(sent(&command(&mut session, list_channels())), ["LIST"]);
    assert!(
        sent(&command(&mut session, list_channels())).is_empty(),
        "a second request waits for the first"
    );
    let motd = feed(&mut session, &[":ergo.test 376 subspace :End of MOTD"]);
    assert!(sent(&motd).is_empty(), "the connection's own ask waits too");

    let refused = feed(
        &mut session,
        &[":ergo.test 263 subspace LIST :Server load is temporarily too heavy"],
    );
    assert!(channel_lists(&refused.events).is_empty());
    assert_eq!(lines(&refused).len(), 1, "the refusal shows");
    assert_eq!(sent(&command(&mut session, list_channels())), ["LIST"]);

    let failed = feed(
        &mut session,
        &[":ergo.test FAIL LIST TEMPORARILY_UNAVAILABLE :Later"],
    );
    assert!(channel_lists(&failed.events).is_empty());
    assert_eq!(sent(&command(&mut session, list_channels())), ["LIST"]);
    let ended = feed(&mut session, &[":ergo.test 323 subspace :End of LIST"]);
    assert_eq!(channel_lists(&ended.events), [&Vec::new()]);
    assert_eq!(sent(&command(&mut session, list_channels())), ["LIST"]);
}

#[test]
fn nothing_is_asked_before_registration() {
    let mut session = Session::new(config(), Carry::default());
    session.start(now());
    assert!(sent(&command(&mut session, list_channels())).is_empty());
}
