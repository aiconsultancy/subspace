//! Items removed from the list: replays and noise stay out; a join, an
//! opened query or a new private message brings them back.

use subspace_client_irc::{Event, LineKind, Target, Typing};

use super::state;
use crate::fixtures::{at, channel, line, member, message, network_id, query};
use crate::state::{AttentionKind, server_key};

#[test]
fn a_closed_query_stays_out_through_replays_until_a_new_message() {
    let mut state = state();
    let mira = query("mira");
    state.apply(Event::Line(message(&mira, "mira", "hi", 1)), at(1));
    assert!(state.remove(&mira));
    assert!(state.buffer(&mira).is_none());
    let replayed = [
        Event::History {
            buffer: mira.clone(),
            lines: vec![message(&mira, "mira", "hi", 1)],
            complete: true,
        },
        Event::Line(message(&mira, "mira", "hi", 1)),
        Event::Replayed(message(&mira, "mira", "from the bouncer", 9)),
        Event::Line(line(&mira, LineKind::Quit, "mira", "Quit: bye", 3)),
        Event::Typing {
            buffer: mira.clone(),
            nick: "mira".into(),
            state: Typing::Active,
        },
    ];
    for event in replayed {
        assert_eq!(state.apply(event, at(6)), None);
    }
    assert!(state.buffer(&mira).is_none(), "replays and noise stay out");

    let back = state.apply(
        Event::Line(message(&query("Mira"), "Mira", "back?", 2)),
        at(7),
    );
    assert_eq!(back.map(|found| found.kind), Some(AttentionKind::Query));
    assert_eq!(
        state.buffer(&query("Mira")).map(|held| held.unread),
        Some(1)
    );
    assert!(
        state.removed.get(&mira).is_none(),
        "the removal is forgotten"
    );
}

#[test]
fn a_closed_empty_query_comes_back_for_any_message_whatever_its_time() {
    let mut state = state();
    let dave = state.open_query(&network_id(), "dave");
    assert!(state.remove(&dave));
    assert_eq!(
        state.removed.get(&dave).map(|found| found.since),
        Some(None)
    );
    // The server's clock may run behind ours: an early time is still new.
    state.apply(Event::Line(message(&dave, "dave", "ping", -60)), at(0));
    assert_eq!(state.buffer(&dave).map(|held| held.unread), Some(1));
}

#[test]
fn the_closing_time_is_the_newest_message_received() {
    let mut state = state();
    let mira = query("mira");
    let lines = [
        message(&mira, "mira", "hi", 1),
        message(&mira, "subspace", "our reply, stamped by us", 50),
        line(&mira, LineKind::Error, "", "Not connected", 60),
    ];
    for held in lines {
        state.apply(Event::Line(held), at(0));
    }
    assert!(state.remove(&mira));
    assert_eq!(
        state.removed.get(&mira).map(|found| found.since),
        Some(Some(at(1))),
        "not our own line or a local error, whose times come from our clock"
    );
    state.apply(Event::Line(message(&mira, "mira", "got it", 2)), at(0));
    assert!(
        state.buffer(&mira).is_some(),
        "a reply after hers comes back"
    );
}

#[test]
fn opening_a_closed_query_brings_it_back() {
    let mut state = state();
    let key = state.open_query(&network_id(), "dave");
    assert_eq!(key.target, Target::Query("dave".into()));
    assert!(state.remove(&key));
    assert!(state.buffer(&key).is_none());
    state.open_query(&network_id(), "dave");
    assert!(state.buffer(&key).is_some());
    assert!(state.removed.get(&key).is_none());
}

#[test]
fn a_left_channel_stays_out_until_joined_again() {
    let mut state = state();
    let room = channel("#subspace");
    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(0),
    );
    assert!(state.remove(&room));
    let after = [
        Event::Line(line(&room, LineKind::Part, "subspace", "", 1)),
        Event::Left {
            buffer: room.clone(),
        },
        Event::Topic {
            buffer: room.clone(),
            topic: "late".into(),
            set_by: None,
        },
        Event::Members {
            buffer: room.clone(),
            members: vec![member("alice", "")],
        },
        Event::Line(message(&room, "alice", "still here?", 2)),
    ];
    for event in after {
        state.apply(event, at(2));
    }
    assert!(
        state.buffer(&room).is_none(),
        "late channel events stay out"
    );

    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(3),
    );
    state.apply(
        Event::Line(line(&room, LineKind::Join, "subspace", "", 3)),
        at(3),
    );
    let held = state.buffer(&room).unwrap();
    assert!(held.joined);
    assert_eq!(held.entries.len(), 1, "our join line shows");
    assert!(
        !state.remove(&server_key(&network_id())),
        "the server stays"
    );
}
