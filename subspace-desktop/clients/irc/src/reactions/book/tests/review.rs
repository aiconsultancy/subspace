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

#[test]
fn reacted_messages_lists_only_this_conversation_s_shown_ones() {
    let mut book = ReactionBook::new();
    let mut before = reaction("a0", "👍", "alice", 1);
    before.buffer.target = Target::Channel("#a".into());
    let mut after = reaction("z9", "👍", "alice", 1);
    after.buffer.target = Target::Channel("#zz".into());
    apply_all(
        &mut book,
        &[
            before,
            after,
            reaction("m1", "👍", "alice", 1),
            reaction("m2", "🎉", "bob", 1),
            removed(reaction("m2", "🎉", "bob", 2)),
        ],
    );
    assert_eq!(
        book.reacted_messages(&room()),
        ["m1"],
        "a message only remembering a removal shows nothing"
    );
}

#[test]
fn a_second_rename_moves_only_the_nick_s_current_holder() {
    let mut book = ReactionBook::new();
    // Robert reacted as robert, then left; Bob later took the nick.
    apply_all(
        &mut book,
        &[
            reaction("m1", "👍", "robert", 1),
            reaction("m1", "🎉", "bob", 2),
        ],
    );
    book.rename(&super::network(), "bob", "robert", None);
    book.rename(&super::network(), "robert", "charlie", None);
    assert_eq!(
        shown(&book, "m1"),
        ["👍:robert", "🎉:charlie"],
        "the earlier robert keeps his reaction"
    );
    // charlie's removal takes only Bob's.
    book.apply(&removed(reaction("m1", "👍", "charlie", 3)));
    assert_eq!(shown(&book, "m1"), ["👍:robert", "🎉:charlie"]);
}

#[test]
fn a_removal_learned_late_moves_a_backdated_place_after_it() {
    let mut book = ReactionBook::new();
    let mut replayed = reaction("m1", "👍", "alice", 1);
    replayed.history = true;
    let mut gone = removed(reaction("m1", "👍", "alice", 3));
    gone.history = true;
    apply_all(
        &mut book,
        &[reaction("m1", "👍", "alice", 5), replayed, gone],
    );
    let summary = book.reactions(&room(), "m1");
    assert_eq!(
        summary[0].reactors[0].time,
        at(5),
        "shown from the addition after the removal"
    );
}
