use chrono::{DateTime, TimeDelta, Utc};

use super::{MAX_MESSAGES, MAX_REACTIONS_PER_MESSAGE, MAX_REACTORS_PER_REACTION, ReactionBook};
use crate::model::{BufferKey, Event, Member, NetworkId, Reaction, ReactionChange, Target};

fn network() -> NetworkId {
    NetworkId("local".into())
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::UNIX_EPOCH + TimeDelta::seconds(seconds)
}

fn reaction(msgid: &str, value: &str, nick: &str, seconds: i64) -> Reaction {
    Reaction {
        buffer: BufferKey {
            network: network(),
            target: Target::Channel("#c".into()),
        },
        msgid: msgid.into(),
        reaction: value.into(),
        change: ReactionChange::Added,
        nick: nick.into(),
        account: None,
        own: nick == "me",
        time: at(seconds),
        id: None,
        history: false,
    }
}

fn removed(mut reaction: Reaction) -> Reaction {
    reaction.change = ReactionChange::Removed;
    reaction
}

/// `reaction:nick,nick` per reaction on `msgid`.
fn shown(book: &ReactionBook, msgid: &str) -> Vec<String> {
    book.reactions(&network(), msgid)
        .iter()
        .map(|summary| {
            let nicks: Vec<&str> = summary.reactors.iter().map(|r| r.nick.as_str()).collect();
            format!("{}:{}", summary.reaction, nicks.join(","))
        })
        .collect()
}

#[test]
fn reactions_group_by_value_in_first_reacted_order() {
    let mut book = ReactionBook::new();
    for held in [
        reaction("m1", "👍", "alice", 3),
        reaction("m1", "🎉", "bob", 2),
        reaction("m1", "👍", "me", 4),
        reaction("m1", "👍", "carol", 1),
    ] {
        book.apply(&held);
    }
    assert_eq!(shown(&book, "m1"), ["👍:carol,alice,me", "🎉:bob"]);
    assert!(book.reacted(&network(), "m1", "👍"));
    assert!(!book.reacted(&network(), "m1", "🎉"));
    assert_eq!(book.reactions(&network(), "m1")[0].count(), 3);
    assert!(!book.reactions(&network(), "m1")[0].stops_agent);
    book.apply(&reaction("m1", "✋\u{1F3FD}", "alice", 5));
    assert!(
        book.reactions(&network(), "m1")[2].stops_agent,
        "Stargate's stop reaction is marked"
    );
    assert!(book.reactions(&network(), "unseen").is_empty());
}

#[test]
fn duplicates_are_ignored_and_unreact_removes_only_that_person() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "alice", 1));
    book.apply(&reaction("m1", "👍", "ALICE", 5));
    book.apply(&reaction("m1", "👍", "bob", 2));
    assert_eq!(shown(&book, "m1"), ["👍:alice,bob"]);
    book.apply(&removed(reaction("m1", "👍", "Alice", 9)));
    book.apply(&removed(reaction("m1", "🎉", "bob", 9)));
    book.apply(&removed(reaction("gone", "👍", "bob", 9)));
    assert_eq!(shown(&book, "m1"), ["👍:bob"]);
    book.apply(&removed(reaction("m1", "👍", "bob", 9)));
    assert!(book.is_empty(), "an emptied message is dropped");
}

#[test]
fn accounts_identify_people_across_nicks() {
    let mut book = ReactionBook::new();
    let mut first = reaction("m1", "👍", "alice", 1);
    first.account = Some("alice".into());
    let mut renamed = reaction("m1", "👍", "alice_away", 2);
    renamed.account = Some("ALICE".into());
    book.apply(&first);
    book.apply(&renamed);
    assert_eq!(shown(&book, "m1"), ["👍:alice"]);
    let mut other = reaction("m1", "👍", "alice", 3);
    other.account = Some("mallory".into());
    book.apply(&other);
    assert_eq!(
        shown(&book, "m1"),
        ["👍:alice,alice"],
        "a different account is someone else"
    );
}

#[test]
fn nick_changes_follow_and_merge() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "bob", 1));
    book.apply(&reaction("m1", "👍", "robert", 2));
    book.apply(&reaction("m2", "🎉", "bob", 3));
    book.observe(&Event::MemberChanged {
        buffer: BufferKey {
            network: network(),
            target: Target::Channel("#c".into()),
        },
        old_nick: "Bob".into(),
        member: Member {
            nick: "robert".into(),
            prefixes: String::new(),
            away: false,
            account: None,
        },
    });
    assert_eq!(shown(&book, "m1"), ["👍:robert"]);
    assert_eq!(shown(&book, "m2"), ["🎉:robert"]);
    book.rename(&NetworkId("elsewhere".into()), "robert", "rob");
    assert_eq!(
        shown(&book, "m2"),
        ["🎉:robert"],
        "other networks are untouched"
    );
}

#[test]
fn our_echo_marks_a_held_reaction_as_ours() {
    let mut book = ReactionBook::new();
    let mut history = reaction("m1", "👍", "me", 1);
    history.own = false;
    book.apply(&history);
    assert!(!book.reacted(&network(), "m1", "👍"));
    book.apply(&reaction("m1", "👍", "me", 2));
    assert!(book.reacted(&network(), "m1", "👍"));
    assert_eq!(book.reactions(&network(), "m1")[0].count(), 1);
}

#[test]
fn invalid_values_from_the_network_are_ignored() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "", "alice", 1));
    book.apply(&reaction("m1", "a\nb", "alice", 1));
    book.apply(&reaction("", "👍", "alice", 1));
    assert!(book.is_empty());
}

#[test]
fn what_is_kept_is_bounded() {
    let mut book = ReactionBook::new();
    for index in 0..MAX_REACTIONS_PER_MESSAGE + 5 {
        book.apply(&reaction("m1", &format!("r{index}"), "alice", 1));
    }
    assert_eq!(
        book.reactions(&network(), "m1").len(),
        MAX_REACTIONS_PER_MESSAGE
    );
    for index in 0..MAX_REACTORS_PER_REACTION + 5 {
        book.apply(&reaction("m2", "👍", &format!("n{index}"), 1));
    }
    assert_eq!(
        book.reactions(&network(), "m2")[0].count(),
        MAX_REACTORS_PER_REACTION
    );
    book.apply(&reaction("m1", "r0", "bob", 2));
    for index in 0..MAX_MESSAGES {
        book.apply(&reaction(&format!("x{index}"), "👍", "alice", 1));
    }
    assert_eq!(book.len(), MAX_MESSAGES);
    assert!(
        book.reactions(&network(), "m2").is_empty(),
        "the least recently changed message went first"
    );
    assert!(book.reactions(&network(), "m1").is_empty(), "then the next");
}
