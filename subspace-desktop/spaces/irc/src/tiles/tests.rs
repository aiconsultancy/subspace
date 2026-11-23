//! The "tiles once" rule: ordering, history loaded above, re-render
//! stability, the line cap, a new recognition and per-conversation scope.

use std::collections::HashSet;

use subspace_client_irc::{BufferKey, NetworkId, Target};

use super::{Sessions, TileOwners, TileRef, Tiles, candidates};
use crate::markup::{PrRef, Ref};

fn issue(id: &str) -> TileRef {
    TileRef::Issue(id.to_owned())
}

fn pr(key: &str) -> TileRef {
    TileRef::PullRequest(key.to_owned())
}

/// Reads `messages` into `tiles` as one sync that holds `held`.
fn read(tiles: &mut Tiles, held: &[i64], messages: &[(i64, Vec<TileRef>)]) {
    let seqs: Vec<i64> = messages.iter().map(|(seq, _)| *seq).collect();
    let unread = tiles.unread(&seqs, 0);
    let found: Vec<_> = messages
        .iter()
        .filter(|(seq, _)| unread.contains(seq))
        .cloned()
        .collect();
    tiles.read(0, &shown(held), &found);
}

fn shown(held: &[i64]) -> HashSet<i64> {
    held.iter().copied().collect()
}

fn shows(tiles: &Tiles, seq: i64, found: &TileRef) -> bool {
    tiles.owners.shows(seq, found)
}

#[test]
fn the_first_message_to_name_a_reference_keeps_its_tile() {
    let mut owners = TileOwners::default();
    let nav = [issue("NAV-2")];
    let both = [issue("NAV-2"), pr("acme/web#492")];
    owners.claim([(1, &nav[..]), (2, &both[..]), (3, &nav[..])]);
    assert!(owners.shows(1, &issue("NAV-2")));
    assert!(!owners.shows(2, &issue("NAV-2")));
    assert!(!owners.shows(3, &issue("NAV-2")));
    assert!(
        owners.shows(2, &pr("acme/web#492")),
        "each reference has its own first"
    );
}

#[test]
fn an_owner_is_never_replaced() {
    let mut owners = TileOwners::default();
    let nav = [issue("NAV-2")];
    owners.claim([(5, &nav[..])]);
    // An older message read later, such as history loaded above.
    owners.claim([(-3, &nav[..])]);
    assert!(owners.shows(5, &issue("NAV-2")));
    assert!(!owners.shows(-3, &issue("NAV-2")));
}

#[test]
fn messages_read_together_go_by_transcript_order() {
    let mut tiles = Tiles::default();
    read(
        &mut tiles,
        &[1, 2, 3],
        &[
            (1, vec![]),
            (2, vec![issue("NAV-2")]),
            (3, vec![issue("NAV-2"), issue("NAV-7")]),
        ],
    );
    assert!(shows(&tiles, 2, &issue("NAV-2")));
    assert!(shows(&tiles, 3, &issue("NAV-7")));
    assert!(!shows(&tiles, 3, &issue("NAV-2")));
}

#[test]
fn history_loaded_above_leaves_the_tile_where_it_was_first_shown() {
    let mut tiles = Tiles::default();
    read(
        &mut tiles,
        &[10, 11],
        &[(10, vec![issue("NAV-2")]), (11, vec![])],
    );
    // History adds older lines with lower sequences, before the held ones.
    let messages = [
        (8, vec![issue("NAV-2")]),
        (9, vec![issue("NAV-9")]),
        (10, vec![issue("NAV-2")]),
        (11, vec![]),
    ];
    read(&mut tiles, &[8, 9, 10, 11], &messages);
    assert!(shows(&tiles, 10, &issue("NAV-2")), "the shown tile stays");
    assert!(
        !shows(&tiles, 8, &issue("NAV-2")),
        "the older mention is a chip"
    );
    assert!(
        shows(&tiles, 9, &issue("NAV-9")),
        "a new reference gets its tile"
    );
}

#[test]
fn new_messages_only_ever_take_references_nobody_owns() {
    let mut tiles = Tiles::default();
    read(&mut tiles, &[1], &[(1, vec![pr("acme/web#492")])]);
    let messages = [
        (1, vec![pr("acme/web#492")]),
        (2, vec![pr("acme/web#492"), pr("acme/web#489")]),
    ];
    read(&mut tiles, &[1, 2], &messages);
    assert!(shows(&tiles, 1, &pr("acme/web#492")));
    assert!(!shows(&tiles, 2, &pr("acme/web#492")));
    assert!(shows(&tiles, 2, &pr("acme/web#489")));
}

#[test]
fn reading_again_changes_nothing() {
    let mut tiles = Tiles::default();
    let messages = [(1, vec![issue("NAV-2")]), (2, vec![issue("NAV-2")])];
    read(&mut tiles, &[1, 2], &messages);
    let before = tiles.owners.clone();
    assert!(
        tiles.unread(&[1, 2], 0).is_empty(),
        "a re-render reads nothing"
    );
    read(&mut tiles, &[1, 2], &messages);
    assert_eq!(tiles.owners, before);
}

#[test]
fn an_owner_no_longer_shown_lets_go_and_a_new_mention_takes_the_tile() {
    let mut tiles = Tiles::default();
    read(
        &mut tiles,
        &[1, 2],
        &[(1, vec![issue("NAV-2")]), (2, vec![issue("NAV-2")])],
    );
    // The cap drops message 1, or it folds into its request: message 2,
    // already drawn as a chip, does not grow a tile.
    read(&mut tiles, &[2], &[(2, vec![issue("NAV-2")])]);
    assert!(!shows(&tiles, 1, &issue("NAV-2")), "let go");
    assert!(
        !shows(&tiles, 2, &issue("NAV-2")),
        "drawn as a chip, stays one"
    );
    assert!(
        !tiles.read.contains(&1),
        "a dropped message is forgotten as read"
    );
    // The next new message that names it shows the tile again.
    read(
        &mut tiles,
        &[2, 3],
        &[(2, vec![issue("NAV-2")]), (3, vec![issue("NAV-2")])],
    );
    assert!(shows(&tiles, 3, &issue("NAV-2")));
}

#[test]
fn a_new_recognition_claims_issues_afresh_and_keeps_pull_requests() {
    let mut tiles = Tiles::default();
    tiles.read(
        0,
        &shown(&[1, 2]),
        &[(1, vec![]), (2, vec![issue("NAV-2"), pr("acme/web#492")])],
    );
    assert_eq!(tiles.unread(&[1, 2], 1), HashSet::from([1, 2]));
    // The new scope recognises NAV-2 in both, and 492 in message 1 too.
    tiles.read(
        1,
        &shown(&[1, 2]),
        &[
            (1, vec![issue("NAV-2"), pr("acme/web#492")]),
            (2, vec![issue("NAV-2"), pr("acme/web#492")]),
        ],
    );
    assert!(
        shows(&tiles, 1, &issue("NAV-2")),
        "claimed within the new scope"
    );
    assert!(
        !shows(&tiles, 2, &issue("NAV-2")),
        "the old scope's claim is gone"
    );
    assert!(
        shows(&tiles, 2, &pr("acme/web#492")),
        "a pull request's owner stays"
    );
    assert!(tiles.unread(&[1, 2], 1).is_empty());
}

#[test]
fn a_sync_with_nothing_new_copies_nothing_and_a_removed_conversation_starts_afresh() {
    let key = BufferKey {
        network: NetworkId("fixture".into()),
        target: Target::Channel("#one".to_owned()),
    };
    let mut sessions = Sessions::default();
    let first = sessions.read(&key, 0, (&[7], &[(7, vec![issue("NAV-2")])]));
    let again = sessions.read(&key, 0, (&[7], &[]));
    assert!(std::rc::Rc::ptr_eq(&first, &again), "no copy of the owners");
    drop((first, again));
    sessions.forget(&key);
    let rejoined = sessions.read(&key, 0, (&[1], &[(1, vec![issue("NAV-2")])]));
    assert!(rejoined.shows(1, &Ref::Issue("NAV-2".into())));
}

#[test]
fn each_conversation_decides_its_own_tiles() {
    let key = |name: &str| BufferKey {
        network: NetworkId("fixture".into()),
        target: Target::Channel(name.to_owned()),
    };
    let mut sessions = Sessions::default();
    let nav = vec![issue("NAV-2")];
    let one = sessions.read(&key("#one"), 0, (&[7], &[(7, nav.clone())]));
    let two = sessions.read(&key("#two"), 0, (&[9], &[(9, nav)]));
    let found = Ref::Issue("NAV-2".into());
    assert!(one.shows(7, &found));
    assert!(two.shows(9, &found), "the same issue has a tile in each");
    assert!(
        sessions.get(&key("#one")).shows(7, &found),
        "kept for the session"
    );
    assert!(!sessions.get(&key("#three")).shows(7, &found));
}

#[test]
fn candidates_are_issues_and_pull_requests_once_each_in_order() {
    let pr_ref = |url: &str| Ref::PullRequest(PrRef::from_url(url).expect("a pull request URL"));
    let found = candidates([
        Ref::Nick("wren".into()),
        Ref::Issue("NAV-7".into()),
        pr_ref("https://github.com/acme/web/pull/492"),
        Ref::Channel("#aurora".into()),
        Ref::Issue("NAV-7".into()),
        pr_ref("https://github.com/Acme/Web/pull/492"),
    ]);
    let keys: Vec<_> = found.iter().filter_map(TileRef::of).collect();
    assert_eq!(keys, vec![issue("NAV-7"), pr("acme/web#492")]);
}

#[test]
fn what_is_in_play_keeps_the_order_a_message_names_it() {
    // A pull request named before eight issues in one message keeps its
    // place under the inspector's cap of eight.
    let mut named = vec![pr("acme/web#492")];
    named.extend((1..=8).map(|n| issue(&format!("NAV-{n}"))));
    let mut owners = TileOwners::default();
    owners.claim([(1, named.as_slice())]);
    let play = owners.in_play();
    assert_eq!(play.first(), Some(&pr("acme/web#492")));
    assert_eq!(play, named);
    assert!(
        play[..8].contains(&pr("acme/web#492")),
        "within the cap of eight"
    );
}

#[test]
fn a_new_recognition_never_gives_an_issue_and_a_kept_pull_request_one_place() {
    let pr = TileRef::PullRequest("acme/web#492".into());
    let issue = TileRef::Issue("NAV-6".into());
    let mut owners = TileOwners::default();
    owners.claim([(1, std::slice::from_ref(&pr))]);
    // A new scope recognises the issue named first in the same message:
    // issues are let go and the message is read again.
    owners.release_issues();
    let now = [issue.clone(), pr.clone()];
    owners.claim([(1, now.as_slice())]);
    let places: std::collections::HashSet<_> = owners.0.values().collect();
    assert_eq!(places.len(), 2, "each claim has a place of its own");
    assert_eq!(
        owners.in_play(),
        [issue.clone(), pr.clone()],
        "as the message names them"
    );
    assert_eq!(owners.in_play_first(1), [issue]);
}

#[test]
fn only_an_issue_counts_as_an_issue_in_play() {
    let mut tiles = Tiles::default();
    assert!(!tiles.has_issue_in_play());
    read(&mut tiles, &[1, 2], &[(1, vec![pr("acme/web#492")])]);
    assert!(!tiles.has_issue_in_play(), "a pull request is not an issue");
    read(&mut tiles, &[1, 2], &[(2, vec![issue("NAV-7")])]);
    assert!(tiles.has_issue_in_play());
}
