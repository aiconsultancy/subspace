//! The book's bounds: shown limits, eviction order and removal-only groups.

use super::super::{
    MAX_MESSAGES, MAX_REACTIONS_PER_MESSAGE, MAX_REACTORS_PER_REACTION, MAX_UNSEEN_MESSAGES,
    MAX_UNSEEN_PER_BUFFER, ReactionBook,
};
use super::review::history;
use super::{reaction, removed, room, shown};
use crate::model::{BufferKey, Target};

/// `count` names, `prefix0` onwards, each with its index as seconds.
fn names(prefix: &str, count: usize) -> impl Iterator<Item = (i64, String)> {
    (0..count).map(move |index| {
        let seconds = i64::try_from(index).unwrap_or_default();
        (seconds, format!("{prefix}{index}"))
    })
}

/// `count` other loaded messages, each with one reaction.
fn fill_seen(book: &mut ReactionBook, count: usize) {
    for (_, msgid) in names("x", count) {
        apply_seen(book, &reaction(&msgid, "👍", "alice", 1));
    }
}

/// Applies `reaction` to a message the book has seen loaded.
fn apply_seen(book: &mut ReactionBook, reaction: &crate::model::Reaction) {
    book.mark_seen(&reaction.buffer, &reaction.msgid);
    book.apply(reaction);
}

#[test]
fn what_is_shown_per_message_is_bounded() {
    let mut book = ReactionBook::new();
    for (_, value) in names("r", MAX_REACTIONS_PER_MESSAGE + 5) {
        book.apply(&reaction("m1", &value, "alice", 1));
    }
    assert_eq!(
        book.reactions(&room(), "m1").len(),
        MAX_REACTIONS_PER_MESSAGE
    );
    for (_, nick) in names("n", MAX_REACTORS_PER_REACTION + 5) {
        book.apply(&reaction("m2", "👍", &nick, 1));
    }
    assert_eq!(
        book.reactions(&room(), "m2")[0].count(),
        MAX_REACTORS_PER_REACTION
    );
}

#[test]
fn eviction_drops_the_least_recently_changed_message() {
    let mut book = ReactionBook::new();
    let first = [
        reaction("m1", "👍", "alice", 1),
        reaction("m2", "👍", "alice", 1),
        reaction("m1", "👍", "bob", 2),
        reaction("m2", "👍", "alice", 1),
    ];
    first.iter().for_each(|held| apply_seen(&mut book, held));
    fill_seen(&mut book, MAX_MESSAGES - 1);
    assert_eq!(book.len(), MAX_MESSAGES);
    assert!(
        shown(&book, "m2").is_empty(),
        "a duplicate does not refresh m2"
    );
    assert!(!shown(&book, "m1").is_empty());
    apply_seen(&mut book, &reaction("last", "👍", "alice", 1));
    assert!(shown(&book, "m1").is_empty());
    assert_eq!(book.len(), MAX_MESSAGES);
}

#[test]
fn a_removal_that_leaves_reactions_refreshes_the_message() {
    let mut book = ReactionBook::new();
    let first = [
        reaction("m1", "👍", "alice", 1),
        reaction("m1", "👍", "bob", 1),
        reaction("m2", "👍", "alice", 1),
        removed(reaction("m1", "👍", "bob", 2)),
    ];
    first.iter().for_each(|held| apply_seen(&mut book, held));
    fill_seen(&mut book, MAX_MESSAGES - 1);
    assert!(shown(&book, "m2").is_empty());
    assert_eq!(shown(&book, "m1"), ["👍:alice"]);
}

#[test]
fn removal_only_groups_give_way_to_new_reactions_oldest_first() {
    let mut book = ReactionBook::new();
    for (seconds, value) in names("junk", MAX_REACTIONS_PER_MESSAGE * 2) {
        book.apply(&removed(reaction("m1", &value, "mallory", seconds)));
    }
    book.apply(&reaction("m1", "👍", "alice", 100));
    assert_eq!(shown(&book, "m1"), ["👍:alice"]);
    book.apply(&reaction("m1", "junk1", "mallory", 0));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:alice"],
        "junk1's removal at 1 is still remembered"
    );
    book.apply(&reaction("m1", "junk0", "mallory", -1));
    assert_eq!(
        shown(&book, "m1"),
        ["junk0:mallory", "👍:alice"],
        "junk0, the oldest, went first, so its older addition shows"
    );
}

#[test]
fn reactivating_remembered_removals_keeps_both_display_limits() {
    let mut book = ReactionBook::new();
    for (_, value) in names("r", MAX_REACTIONS_PER_MESSAGE * 2) {
        book.apply(&removed(reaction("m1", &value, "mallory", 1)));
        book.apply(&reaction("m1", &value, "mallory", 2));
    }
    assert_eq!(
        book.reactions(&room(), "m1").len(),
        MAX_REACTIONS_PER_MESSAGE
    );
    for (_, nick) in names("n", MAX_REACTORS_PER_REACTION * 2) {
        book.apply(&removed(reaction("m2", "👍", &nick, 1)));
        book.apply(&reaction("m2", "👍", &nick, 2));
    }
    assert_eq!(
        book.reactions(&room(), "m2")[0].count(),
        MAX_REACTORS_PER_REACTION
    );
}

#[test]
fn a_refused_addition_evicts_no_remembered_removal() {
    let mut book = ReactionBook::new();
    for (seconds, value) in names("s", MAX_REACTIONS_PER_MESSAGE) {
        book.apply(&reaction("m1", &value, "alice", 1));
        book.apply(&removed(reaction(
            "m1",
            &value.replace('s', "j"),
            "mallory",
            10 + seconds,
        )));
    }
    book.apply(&reaction("m1", "new", "bob", 50));
    book.apply(&removed(reaction("m1", "s0", "alice", 60)));
    book.apply(&reaction("m1", "j0", "mallory", 5));
    assert!(
        !shown(&book, "m1")
            .iter()
            .any(|group| group.starts_with("j0")),
        "j0's removal at 10 was still remembered"
    );
}

#[test]
fn removals_give_way_by_when_they_were_removed() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "alice", 1));
    book.apply(&removed(reaction("m1", "👍", "alice", 500)));
    for (seconds, nick) in names("n", MAX_REACTORS_PER_REACTION * 2 - 1) {
        book.apply(&removed(reaction("m1", "👍", &nick, 10 + seconds)));
    }
    book.apply(&removed(reaction("m1", "👍", "zed", 600)));
    book.apply(&reaction("m1", "👍", "alice", 50));
    assert!(
        shown(&book, "m1").is_empty(),
        "alice's recent removal outlived older ones"
    );
}

#[test]
fn reactions_to_unseen_messages_have_their_own_budget() {
    let mut book = ReactionBook::new();
    book.observe(&history(room(), &["loaded"]));
    book.apply(&reaction("loaded", "👍", "alice", 1));
    for (_, msgid) in names("ghost", MAX_MESSAGES + 10) {
        book.apply(&reaction(&msgid, "👍", "mallory", 2));
    }
    assert_eq!(
        shown(&book, "loaded"),
        ["👍:alice"],
        "reactions to unseen messages never displace a loaded one"
    );
    assert_eq!(book.len(), 1 + MAX_UNSEEN_PER_BUFFER);
    let newest = format!("ghost{}", MAX_MESSAGES + 9);
    assert!(!shown(&book, &newest).is_empty(), "the newest are kept");
    book.observe(&history(room(), &[&newest]));
    book.apply(&reaction(&newest, "🎉", "bob", 4));
    assert_eq!(
        shown(&book, &newest),
        ["👍:mallory", "🎉:bob"],
        "promoted once loaded"
    );
    for (_, name) in names("#c", MAX_UNSEEN_MESSAGES) {
        let mut elsewhere = reaction("ghost", "👍", "mallory", 3);
        elsewhere.buffer = BufferKey {
            target: Target::Channel(name),
            ..room()
        };
        book.apply(&elsewhere);
    }
    assert_eq!(book.len(), 2 + MAX_UNSEEN_MESSAGES, "a total bound too");
    assert_eq!(shown(&book, "loaded"), ["👍:alice"]);
    assert_eq!(shown(&book, &newest).len(), 2, "a promoted message stays");
}
