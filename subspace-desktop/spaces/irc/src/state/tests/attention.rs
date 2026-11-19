//! Which events raise attention: the decision behind IRC notifications.
//! Whether the buffer is in view is the hub's part (`tests::notifications`).

use subspace_client_irc::{BufferKey, Event, Line, LineKind};

use super::super::{Attention, AttentionKind, server_key};
use super::state;
use crate::fixtures::{at, channel, line, message, network_id, query};

/// A message the runtime marked as mentioning us, as it does for `@subspace`.
fn tagged(buffer: &BufferKey, from: &str, text: &str, minute: i64) -> Line {
    let mut line = message(buffer, from, text, minute);
    line.highlight = true;
    line
}

fn attention(buffer: &BufferKey, kind: AttentionKind, from: &str) -> Option<Attention> {
    Some(Attention {
        buffer: buffer.clone(),
        kind,
        from: Some(from.into()),
    })
}

#[test]
fn a_tag_notifies_with_its_sender_and_an_untagged_line_does_not() {
    let mut state = state();
    let room = channel("#subspace");
    let plain = message(&room, "alice", "morning, everyone", 1);
    assert_eq!(state.apply(Event::Line(plain), at(1)), None);
    let tag = tagged(&room, "alice", "@subspace can you look?", 2);
    assert_eq!(
        state.apply(Event::Line(tag), at(2)),
        attention(&room, AttentionKind::Highlight, "alice")
    );
}

#[test]
fn private_messages_notify_without_a_tag() {
    let mut state = state();
    let carol = query("carol");
    let private = message(&carol, "carol", "are you there?", 1);
    assert_eq!(
        state.apply(Event::Line(private), at(1)),
        attention(&carol, AttentionKind::Query, "carol")
    );
}

#[test]
fn our_own_lines_never_notify_even_when_they_name_us() {
    let mut state = state();
    let room = channel("#subspace");
    let own = tagged(&room, "subspace", "@subspace note to self", 1);
    assert!(own.own, "the fixture marks our nick's lines as own");
    assert_eq!(state.apply(Event::Line(own), at(1)), None);
    let reply = message(&query("carol"), "subspace", "on my way", 2);
    assert_eq!(state.apply(Event::Line(reply), at(2)), None);
}

/// A history page for `buffer`, as a `chathistory` batch delivers it.
fn page(buffer: &BufferKey, lines: Vec<Line>) -> Event {
    Event::History {
        buffer: buffer.clone(),
        lines,
        complete: false,
    }
}

#[test]
fn history_never_notifies_even_for_tags_and_private_messages() {
    let mut state = state();
    let room = channel("#subspace");
    let carol = query("carol");
    let playback = vec![
        tagged(&room, "alice", "@subspace ping", 1),
        line(&room, LineKind::Join, "bob", "joined", 2),
        tagged(&room, "bob", "@subspace and again", 3),
    ];
    assert_eq!(state.apply(page(&room, playback), at(10)), None);
    let private = vec![message(&carol, "carol", "are you there?", 4)];
    assert_eq!(state.apply(page(&carol, private), at(10)), None);
}

#[test]
fn a_bouncer_replay_counts_as_unread_but_never_notifies() {
    let mut state = state();
    let room = channel("#subspace");
    let replay = tagged(&room, "alice", "@subspace while you were away", 1);
    assert_eq!(state.apply(Event::Replayed(replay), at(5)), None);
    let buffer = state.buffer(&room).unwrap();
    assert_eq!(
        (buffer.unread, buffer.highlights),
        (1, 1),
        "the badges still show it"
    );
}

#[test]
fn a_line_the_read_marker_covers_does_not_notify() {
    let mut state = state();
    let room = channel("#subspace");
    state.apply(
        Event::Joined {
            buffer: room.clone(),
        },
        at(0),
    );
    // Another client read up to minute 10; a server replays older lines live.
    let marker = Event::ReadMarker {
        buffer: room.clone(),
        time: at(10),
    };
    state.apply(marker, at(11));
    let older = tagged(&room, "alice", "@subspace earlier", 9);
    assert_eq!(state.apply(Event::Line(older), at(11)), None);
    let same = tagged(&room, "alice", "@subspace at the marker", 10);
    assert_eq!(state.apply(Event::Line(same), at(11)), None);
    let newer = tagged(&room, "alice", "@subspace after it", 12);
    assert_eq!(
        state.apply(Event::Line(newer), at(12)),
        attention(&room, AttentionKind::Highlight, "alice")
    );
}

#[test]
fn a_line_already_held_does_not_notify_again() {
    let mut state = state();
    let room = channel("#subspace");
    let mut tag = tagged(&room, "alice", "@subspace once", 1);
    tag.id = Some("m1".into());
    assert!(state.apply(Event::Line(tag.clone()), at(1)).is_some());
    assert_eq!(state.apply(Event::Line(tag), at(2)), None, "same msgid");
}

#[test]
fn a_server_buffer_notice_that_names_us_does_not_notify() {
    let mut state = state();
    let server = server_key(&network_id());
    let mut notice = line(
        &server,
        LineKind::Notice,
        "NickServ",
        "You are now identified for subspace",
        1,
    );
    notice.highlight = true;
    assert_eq!(state.apply(Event::Line(notice), at(1)), None);
    assert_eq!(state.attention_count(), 0, "the badge agrees");
}
