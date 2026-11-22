//! Cases from the client layer's review: identity after nick changes and
//! case mapping changes, live against history at the same time, and
//! backdating across a known removal.

use std::collections::BTreeMap;

use super::super::ReactionBook;
use super::{apply_all, at, reaction, removed, renamed, room, shown};
use crate::model::{BufferKey, Casemapping, Event, Line, LineKind, Target};

#[test]
fn after_an_accountless_rename_the_current_holder_of_the_nick_is_matched() {
    let mut book = ReactionBook::new();
    apply_all(
        &mut book,
        &[
            reaction("m1", "👍", "robert", 1),
            reaction("m1", "👍", "bob", 2),
        ],
    );
    book.observe(&renamed("bob", "robert", None));
    book.apply(&removed(reaction("m1", "👍", "robert", 3)));
    let reactors = book.reactions(&room(), "m1");
    let times: Vec<_> = reactors[0].reactors.iter().map(|r| r.time).collect();
    assert_eq!(
        times,
        [at(1)],
        "the removal takes the reaction of the person who now holds the nick"
    );
}

#[test]
fn nicks_compare_under_the_current_case_mapping() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "[x]", 1));
    let mut later = removed(reaction("m1", "👍", "{x}", 2));
    later.casemapping = Casemapping::Rfc1459;
    book.apply(&later);
    assert!(
        shown(&book, "m1").is_empty(),
        "under RFC 1459 the two spellings are one person"
    );
}

#[test]
fn a_history_operation_never_overrides_a_live_one_at_the_same_time() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "alice", 5));
    let mut replayed = removed(reaction("m1", "👍", "alice", 5));
    replayed.history = true;
    book.apply(&replayed);
    assert_eq!(shown(&book, "m1"), ["👍:alice"], "the live addition stands");
    book.apply(&removed(reaction("m1", "👍", "alice", 5)));
    assert!(
        shown(&book, "m1").is_empty(),
        "a live operation at the same time still counts"
    );
}

#[test]
fn an_old_addition_never_backdates_across_a_known_removal() {
    let mut book = ReactionBook::new();
    apply_all(
        &mut book,
        &[
            reaction("m1", "👍", "alice", 1),
            reaction("m1", "👍", "bob", 2),
            removed(reaction("m1", "👍", "alice", 3)),
            reaction("m1", "👍", "alice", 5),
        ],
    );
    let mut replayed = reaction("m1", "👍", "alice", 1);
    replayed.history = true;
    book.apply(&replayed);
    assert_eq!(
        shown(&book, "m1"),
        ["👍:bob,alice"],
        "alice's place is her re-addition at 5"
    );
}

fn line(buffer: BufferKey, id: &str) -> Line {
    Line {
        buffer,
        id: Some(id.into()),
        reply_to: None,
        client_tags: BTreeMap::new(),
        time: at(0),
        kind: LineKind::Privmsg,
        from: Some("alice".into()),
        account: None,
        text: "hello".into(),
        highlight: false,
        own: false,
    }
}

#[test]
fn forgetting_a_conversation_drops_its_reactions() {
    let mut book = ReactionBook::new();
    let mut elsewhere = reaction("m1", "👍", "alice", 1);
    elsewhere.buffer.target = Target::Channel("#d".into());
    apply_all(
        &mut book,
        &[reaction("m1", "👍", "alice", 1), elsewhere.clone()],
    );
    book.observe(&Event::Line(line(room(), "m1")));
    assert_eq!(book.reacted_messages(&room()), ["m1"]);
    book.forget(&room());
    assert!(book.reacted_messages(&room()).is_empty());
    assert!(shown(&book, "m1").is_empty());
    assert_eq!(book.reactions(&elsewhere.buffer, "m1").len(), 1);
    assert_eq!(book.len(), 1);
}

pub(super) fn history(buffer: BufferKey, ids: &[&str]) -> Event {
    Event::History {
        lines: ids.iter().map(|id| line(buffer.clone(), id)).collect(),
        buffer,
        complete: false,
    }
}
