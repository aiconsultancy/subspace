//! Which messages belong to a thread and its reply count; the timeline and
//! the chosen thread's lifetime are in the submodules.

use subspace_client_irc::{BufferKey, Event};

use super::{Thread, build};
use crate::fixtures::{at, channel, message};
use crate::state::IrcState;
use crate::thread_status::tests::{follow_up, joined, live, progress, request, room, said, ship};

/// The room's thread of `root`, as built now.
fn thread(state: &IrcState, root: &str) -> Option<Thread> {
    build(&room(), state.buffer(&room())?, root)
}

/// The message IDs of the thread's replies, in order.
fn reply_ids(thread: &Thread) -> Vec<&str> {
    thread
        .replies
        .iter()
        .filter_map(|entry| entry.line.id.as_deref())
        .collect()
}

/// `text` from `nick` with message ID `id`, replying to `parent`.
fn reply(
    buffer: &BufferKey,
    nick: &str,
    id: &str,
    parent: &str,
    minute: i64,
) -> subspace_client_irc::Line {
    let mut line = message(buffer, nick, "A reply.", minute);
    line.id = Some(id.into());
    line.reply_to = Some(parent.into());
    line
}

mod lifetime;
mod timeline;

#[test]
fn a_reply_belongs_by_its_server_reply_tag_never_by_its_text() {
    let mut state = joined(vec![ship()]);
    let mut mention = message(&room(), "mira", "Re: first, about the first thread", 6);
    mention.id = Some("mention".into());
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            reply(&room(), "mira", "r1", "first", 2),
            reply(&room(), "tavi", "r2", "r1", 3),
            reply(&room(), "tavi", "elsewhere", "other", 4),
            said(&room(), Some("first"), "answer", 5),
            mention,
        ],
    );
    let found = thread(&state, "first").unwrap();
    assert_eq!(found.root.line.id.as_deref(), Some("first"));
    assert_eq!(
        reply_ids(&found),
        ["r1", "r2", "answer"],
        "a reply to a reply belongs; a mention or another thread does not"
    );
}

#[test]
fn a_reply_in_another_conversation_or_a_missing_root_is_not_a_thread() {
    let mut state = joined(vec![ship()]);
    let other = channel("#lounge");
    state.apply(
        Event::Joined {
            buffer: other.clone(),
        },
        at(0),
    );
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            reply(&other, "mira", "foreign", "first", 2),
        ],
    );
    assert!(thread(&state, "first").unwrap().replies.is_empty());
    assert_eq!(thread(&state, "nowhere"), None, "the root must be held");
}

#[test]
fn a_message_no_agent_reported_on_has_no_timeline() {
    let mut state = joined(vec![ship()]);
    let mut root = message(&room(), "mira", "Morning, all.", 1);
    root.id = Some("hello".into());
    live(&mut state, [root, reply(&room(), "tavi", "r1", "hello", 2)]);
    let found = thread(&state, "hello").unwrap();
    assert_eq!(found.timeline, None);
    assert_eq!(reply_ids(&found), ["r1"]);
}

#[test]
fn the_reply_count_is_the_thread_s_reply_count() {
    let mut state = joined(vec![ship()]);
    live(
        &mut state,
        [
            request(&room(), "first", 1),
            progress(&room(), "first", "accepted", 2),
            reply(&room(), "mira", "r1", "first", 3),
            follow_up(&room(), "again", "first", 4),
            progress(&room(), "first", "completed", 5),
        ],
    );
    let held = state.buffer(&room()).unwrap();
    let counts = crate::transcript::replies::counts(&room(), held);
    let root = held
        .entries
        .iter()
        .find(|entry| entry.line.id.as_deref() == Some("first"))
        .unwrap();
    let found = thread(&state, "first").unwrap();
    assert_eq!(
        counts
            .get(&root.seq)
            .map(crate::transcript::replies::Summary::count),
        Some(found.replies.len())
    );
    assert_eq!(found.replies.len(), 2, "progress is not counted");
}
