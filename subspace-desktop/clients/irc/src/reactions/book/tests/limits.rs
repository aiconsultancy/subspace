//! The book's bounds: shown limits, eviction order and removal-only groups.

use super::super::{
    MAX_MESSAGES, MAX_REACTIONS_PER_MESSAGE, MAX_REACTORS_PER_REACTION, ReactionBook,
};
use super::{reaction, removed, room, shown};

#[test]
fn what_is_shown_per_message_is_bounded() {
    let mut book = ReactionBook::new();
    for index in 0..MAX_REACTIONS_PER_MESSAGE + 5 {
        book.apply(&reaction("m1", &format!("r{index}"), "alice", 1));
    }
    assert_eq!(
        book.reactions(&room(), "m1").len(),
        MAX_REACTIONS_PER_MESSAGE
    );
    for index in 0..MAX_REACTORS_PER_REACTION + 5 {
        book.apply(&reaction("m2", "👍", &format!("n{index}"), 1));
    }
    assert_eq!(
        book.reactions(&room(), "m2")[0].count(),
        MAX_REACTORS_PER_REACTION
    );
}

#[test]
fn eviction_drops_the_least_recently_changed_message() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "alice", 1));
    book.apply(&reaction("m2", "👍", "alice", 1));
    book.apply(&reaction("m1", "👍", "bob", 2));
    book.apply(&reaction("m2", "👍", "alice", 1));
    for index in 0..MAX_MESSAGES - 1 {
        book.apply(&reaction(&format!("x{index}"), "👍", "alice", 1));
    }
    assert_eq!(book.len(), MAX_MESSAGES);
    assert!(
        shown(&book, "m2").is_empty(),
        "a duplicate does not refresh m2"
    );
    assert!(!shown(&book, "m1").is_empty());
    book.apply(&reaction("last", "👍", "alice", 1));
    assert!(shown(&book, "m1").is_empty());
    assert_eq!(book.len(), MAX_MESSAGES);
}

#[test]
fn a_removal_that_leaves_reactions_refreshes_the_message() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "alice", 1));
    book.apply(&reaction("m1", "👍", "bob", 1));
    book.apply(&reaction("m2", "👍", "alice", 1));
    book.apply(&removed(reaction("m1", "👍", "bob", 2)));
    for index in 0..MAX_MESSAGES - 1 {
        book.apply(&reaction(&format!("x{index}"), "👍", "alice", 1));
    }
    assert!(shown(&book, "m2").is_empty());
    assert_eq!(shown(&book, "m1"), ["👍:alice"]);
}

#[test]
fn removal_only_groups_give_way_to_new_reactions() {
    let mut book = ReactionBook::new();
    for index in 0..MAX_REACTIONS_PER_MESSAGE * 2 {
        let seconds = i64::try_from(index).unwrap_or_default();
        book.apply(&removed(reaction(
            "m1",
            &format!("junk{index}"),
            "mallory",
            seconds,
        )));
    }
    book.apply(&reaction("m1", "👍", "alice", 100));
    assert_eq!(shown(&book, "m1"), ["👍:alice"]);
    book.apply(&reaction("m1", "junk0", "bob", 101));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:alice", "junk0:bob"],
        "the oldest went first"
    );
}
