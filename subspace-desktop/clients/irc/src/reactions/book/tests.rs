use chrono::{DateTime, TimeDelta, Utc};

use super::{MAX_MESSAGES, MAX_REACTIONS_PER_MESSAGE, MAX_REACTORS_PER_REACTION, ReactionBook};
use crate::model::{
    BufferKey, Casemapping, Event, Member, NetworkId, Reaction, ReactionChange, Target,
};
use crate::test_support;

fn network() -> NetworkId {
    NetworkId("local".into())
}

fn room() -> BufferKey {
    BufferKey {
        network: network(),
        target: Target::Channel("#c".into()),
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::UNIX_EPOCH + TimeDelta::seconds(seconds)
}

fn reaction(msgid: &str, value: &str, nick: &str, seconds: i64) -> Reaction {
    let mut reaction = test_support::reaction(room(), msgid, value, nick);
    reaction.own = nick == "me";
    reaction.time = at(seconds);
    reaction
}

fn with_account(mut reaction: Reaction, account: &str) -> Reaction {
    reaction.account = Some(account.into());
    reaction
}

fn removed(mut reaction: Reaction) -> Reaction {
    reaction.change = ReactionChange::Removed;
    reaction
}

/// `reaction:nick,nick` per reaction on `msgid` in `#c`.
fn shown(book: &ReactionBook, msgid: &str) -> Vec<String> {
    book.reactions(&room(), msgid)
        .iter()
        .map(|summary| {
            let nicks: Vec<&str> = summary.reactors.iter().map(|r| r.nick.as_str()).collect();
            format!("{}:{}", summary.reaction, nicks.join(","))
        })
        .collect()
}

fn apply_all(book: &mut ReactionBook, reactions: &[Reaction]) {
    reactions.iter().for_each(|reaction| book.apply(reaction));
}

#[test]
fn reactions_group_by_value_in_first_reacted_order() {
    let mut book = ReactionBook::new();
    apply_all(
        &mut book,
        &[
            reaction("m1", "👍", "alice", 3),
            reaction("m1", "🎉", "bob", 2),
            reaction("m1", "👍", "me", 4),
            reaction("m1", "👍", "carol", 1),
        ],
    );
    assert_eq!(shown(&book, "m1"), ["👍:carol,alice,me", "🎉:bob"]);
    assert!(book.reacted(&room(), "m1", "👍"));
    assert!(!book.reacted(&room(), "m1", "🎉"));
    assert!(!book.reactions(&room(), "m1")[0].stops_agent);
    book.apply(&reaction("m1", "✋\u{1F3FD}", "alice", 5));
    assert!(book.reactions(&room(), "m1")[2].stops_agent);
    assert!(book.reactions(&room(), "unseen").is_empty());
}

#[test]
fn a_message_id_counts_only_in_its_own_buffer() {
    let mut book = ReactionBook::new();
    let mut private = reaction("m1", "🛑", "mallory", 1);
    private.buffer.target = Target::Query("mallory".into());
    book.apply(&private);
    assert!(
        shown(&book, "m1").is_empty(),
        "a private reaction stays private"
    );
    assert_eq!(book.reactions(&private.buffer, "m1").len(), 1);
}

#[test]
fn duplicates_are_ignored_and_unreact_removes_only_that_person() {
    let mut book = ReactionBook::new();
    apply_all(
        &mut book,
        &[
            reaction("m1", "👍", "alice", 1),
            reaction("m1", "👍", "ALICE", 5),
            reaction("m1", "👍", "bob", 2),
            removed(reaction("m1", "👍", "Alice", 9)),
            removed(reaction("m1", "🎉", "bob", 9)),
        ],
    );
    assert_eq!(shown(&book, "m1"), ["👍:bob"]);
    book.apply(&removed(reaction("m1", "👍", "bob", 9)));
    assert!(shown(&book, "m1").is_empty());
}

#[test]
fn removing_the_earliest_person_reorders_the_groups() {
    let mut book = ReactionBook::new();
    apply_all(
        &mut book,
        &[
            reaction("m1", "👍", "alice", 1),
            reaction("m1", "🎉", "bob", 2),
            reaction("m1", "👍", "carol", 3),
            removed(reaction("m1", "👍", "alice", 4)),
        ],
    );
    assert_eq!(shown(&book, "m1"), ["🎉:bob", "👍:carol"]);
}

#[test]
fn older_replay_never_overwrites_a_newer_change() {
    let mut book = ReactionBook::new();
    book.apply(&removed(reaction("m1", "👍", "alice", 5)));
    let mut replayed = reaction("m1", "👍", "alice", 1);
    replayed.history = true;
    book.apply(&replayed);
    assert!(
        shown(&book, "m1").is_empty(),
        "the older addition stays removed"
    );
    book.apply(&reaction("m1", "👍", "alice", 6));
    assert_eq!(shown(&book, "m1"), ["👍:alice"], "a newer addition counts");
    book.apply(&removed(reaction("m1", "👍", "alice", 3)));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:alice"],
        "an older removal does not"
    );
}

#[test]
fn accounts_identify_people_and_guard_their_entries() {
    let mut book = ReactionBook::new();
    book.apply(&with_account(reaction("m1", "👍", "alice", 1), "alice"));
    book.apply(&with_account(
        reaction("m1", "👍", "alice_away", 2),
        "ALICE",
    ));
    assert_eq!(shown(&book, "m1"), ["👍:alice"]);
    book.apply(&with_account(reaction("m1", "👍", "alice", 3), "mallory"));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:alice,alice"],
        "another account is someone else"
    );
    book.apply(&removed(reaction("m1", "👍", "alice", 4)));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:alice,alice"],
        "a removal without an account cannot take an account's reaction"
    );
}

#[test]
fn nicks_compare_with_the_networks_case_mapping() {
    let mut book = ReactionBook::new();
    book.apply(&reaction("m1", "👍", "[x]", 1));
    book.apply(&reaction("m1", "👍", "{x}", 2));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:[x],{x}"],
        "ASCII keeps brackets apart"
    );
    let mut rfc = reaction("m2", "👍", "{y}", 3);
    rfc.casemapping = Casemapping::Rfc1459;
    book.apply(&rfc);
    rfc.nick = "[y]".into();
    book.apply(&removed(rfc));
    assert!(
        shown(&book, "m2").is_empty(),
        "RFC 1459 folds them together"
    );
}

fn renamed(old: &str, new: &str, account: Option<&str>) -> Event {
    Event::MemberChanged {
        buffer: room(),
        old_nick: old.into(),
        member: Member {
            nick: new.into(),
            prefixes: String::new(),
            away: false,
            account: account.map(str::to_owned),
        },
    }
}

#[test]
fn nick_changes_follow_people_without_merging_strangers() {
    let mut book = ReactionBook::new();
    apply_all(
        &mut book,
        &[
            reaction("m1", "👍", "bob", 1),
            reaction("m1", "👍", "robert", 2),
            reaction("m2", "🎉", "bob", 3),
        ],
    );
    book.observe(&renamed("Bob", "robert", None));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:robert,robert"],
        "two people without accounts are not merged"
    );
    assert_eq!(shown(&book, "m2"), ["🎉:robert"]);
    book.rename(&NetworkId("elsewhere".into()), "robert", "rob", None);
    assert_eq!(
        shown(&book, "m2"),
        ["🎉:robert"],
        "other networks are untouched"
    );
}

#[test]
fn a_rename_skips_entries_with_another_account() {
    let mut book = ReactionBook::new();
    book.apply(&with_account(reaction("m1", "👍", "carol", 1), "carol"));
    book.apply(&reaction("m2", "👍", "carol", 2));
    book.observe(&renamed("carol", "caz", Some("mallory")));
    assert_eq!(
        shown(&book, "m1"),
        ["👍:carol"],
        "carol's account is not mallory's"
    );
    assert_eq!(
        shown(&book, "m2"),
        ["👍:caz"],
        "an unknown account follows the nick"
    );
}

#[test]
fn our_echo_marks_a_held_reaction_as_ours() {
    let mut book = ReactionBook::new();
    let mut history = reaction("m1", "👍", "me", 1);
    history.own = false;
    book.apply(&history);
    assert!(!book.reacted(&room(), "m1", "👍"));
    book.apply(&reaction("m1", "👍", "me", 2));
    assert!(book.reacted(&room(), "m1", "👍"));
    assert_eq!(book.reactions(&room(), "m1")[0].count(), 1);
    book.apply(&removed(reaction("m1", "👍", "me", 3)));
    assert!(
        !book.reacted(&room(), "m1", "👍"),
        "our removal matches our entry"
    );
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
