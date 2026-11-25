//! The snapshot joined with the officer's events: no gap, no duplicate,
//! and a fetch again whenever the events cannot be trusted to follow on.

use std::time::Duration;

use super::*;
use crate::activity::{Fetch, MAX_HELD_BYTES, MAX_HELD_EVENTS};
use crate::{FleetState, Link};

const POLLED: FleetState = FleetState::Followed(Link::Polling);

#[test]
fn the_snapshot_and_events_join_without_a_gap_or_a_duplicate() {
    let mut join = ActivityJoin::new();
    // Subscribed: the stream connects and events wait for the snapshot.
    let early = vec![
        item_event("g1", 8, "i4", "covered"),
        item_event("g1", 10, "i4", "Hello there"),
    ];
    assert!(join.absorb(news(Some(1), early, 2)));
    assert!(join.start_fetch());
    assert!(!join.start_fetch(), "one fetch at a time");
    join.absorb(news(Some(1), vec![item_event("g1", 12, "i6", "Next")], 3));
    join.finish_fetch(Ok(snapshot("g1", 9)));
    assert_eq!(join.fetch(), Fetch::Idle);
    assert_eq!(words_of(&join, "i4").as_deref(), Some("Hello there"));
    assert_eq!(words_of(&join, "i6").as_deref(), Some("Next"));
    // A skipped seq is not a gap.
    assert!(join.absorb(news(
        Some(1),
        vec![item_event("g1", 20, "i6", "Next one")],
        4
    )));
    assert_eq!(join.fetch(), Fetch::Idle);
    let items = &join.activity().unwrap().turns[1].items;
    assert_eq!(
        items.iter().filter(|item| item.base().id == "i6").count(),
        1
    );
    assert_eq!(words_of(&join, "i6").as_deref(), Some("Next one"));
    assert_eq!(join.activity().unwrap().cursor.seq, 20);
    assert_eq!(join.next(), 4);
}

#[test]
fn a_new_turn_and_its_end_are_upserts() {
    let mut join = ActivityJoin::new();
    join.absorb(news(Some(1), Vec::new(), 0));
    join.start_fetch();
    join.finish_fetch(Ok(snapshot("g1", 9)));
    let summary = |seq: u64, status: &str| {
        let mut turn = turn("t-next", seq, status, Value::Null);
        turn.as_object_mut().unwrap().remove("items");
        turn
    };
    let started = event(
        "g1",
        10,
        json!({"type": "turn-started", "turn": summary(10, "running")}),
    );
    let ended = event(
        "g1",
        11,
        json!({"type": "turn-ended", "turn": summary(11, "completed")}),
    );
    join.absorb(news(Some(1), vec![started, ended], 2));
    let turns = &join.activity().unwrap().turns;
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[2].summary.status, TurnStatus::Completed);
    assert_eq!(join.fetch(), Fetch::Idle);
}

fn joined() -> ActivityJoin {
    let mut join = ActivityJoin::new();
    join.absorb(news(Some(1), Vec::new(), 0));
    join.start_fetch();
    join.finish_fetch(Ok(snapshot("g1", 9)));
    join
}

#[test]
fn a_seq_that_fails_to_rise_fetches_again() {
    let mut join = joined();
    join.absorb(news(Some(1), vec![item_event("g1", 12, "i4", "a")], 1));
    assert_eq!(join.fetch(), Fetch::Idle);
    join.absorb(news(Some(1), vec![item_event("g1", 12, "i4", "b")], 2));
    assert_eq!(join.fetch(), Fetch::Wanted);
    assert!(join.start_fetch());
    join.finish_fetch(Ok(snapshot("g1", 30)));
    assert_eq!(join.fetch(), Fetch::Idle);
}

#[test]
fn a_new_generation_fetches_again_and_its_events_wait() {
    let mut join = joined();
    join.absorb(news(
        Some(1),
        vec![item_event("g2", 3, "i4", "after restart")],
        1,
    ));
    assert_eq!(join.fetch(), Fetch::Wanted);
    assert_eq!(words_of(&join, "i4").as_deref(), Some("Hello"));
    join.start_fetch();
    join.absorb(news(Some(1), vec![item_event("g2", 5, "i4", "later")], 2));
    join.finish_fetch(Ok(snapshot("g2", 4)));
    assert_eq!(join.fetch(), Fetch::Idle);
    assert_eq!(words_of(&join, "i4").as_deref(), Some("later"));
}

#[test]
fn a_reconnect_or_a_loss_fetches_again_even_during_a_fetch() {
    let mut join = joined();
    join.absorb(news(None, Vec::new(), 0));
    assert_eq!(join.fetch(), Fetch::Wanted);
    join.set_fleet_state(&POLLED);
    join.start_fetch();
    join.set_fleet_state(&FleetState::Followed(Link::Reconnecting));
    // The stream comes back while the fetch runs: fetch once more after.
    join.absorb(news(Some(2), Vec::new(), 0));
    join.finish_fetch(Ok(snapshot("g1", 9)));
    assert_eq!(join.fetch(), Fetch::Wanted);
    join.start_fetch();
    join.finish_fetch(Ok(snapshot("g1", 9)));
    assert_eq!(join.fetch(), Fetch::Idle);
    let lost = ActivityNews {
        connection: Some(2),
        missed: true,
        ..ActivityNews::default()
    };
    join.absorb(lost);
    assert_eq!(join.fetch(), Fetch::Wanted);
}

#[test]
fn an_item_of_an_unknown_turn_fetches_again() {
    let mut join = joined();
    let mut stray = item_event("g1", 15, "i1", "x");
    stray.turn_id = "t-elsewhere".into();
    join.absorb(news(Some(1), vec![stray], 1));
    assert_eq!(join.fetch(), Fetch::Wanted);
}

#[test]
fn the_snapshot_waits_for_the_stream_to_answer() {
    let mut join = ActivityJoin::new();
    assert_eq!(join.fetch(), Fetch::Wanted);
    assert!(!join.start_fetch(), "no stream has answered yet");
    join.absorb(news(Some(1), Vec::new(), 0));
    assert!(join.start_fetch());
    join.finish_fetch(Ok(snapshot("g1", 9)));
    // The stream closes: the refetch waits for the next to answer.
    join.absorb(news(None, Vec::new(), 0));
    assert_eq!(join.fetch(), Fetch::Wanted);
    assert!(!join.start_fetch());
    join.absorb(news(Some(2), Vec::new(), 0));
    assert!(join.start_fetch());
    // A polled fleet, or a refused subscription, fetches without a stream.
    // Connecting or reconnecting is not polling: the fetch still waits.
    for link in [Link::Connecting, Link::Reconnecting, Link::Unreachable] {
        let mut waiting = ActivityJoin::new();
        waiting.set_fleet_state(&FleetState::Followed(link));
        assert!(!waiting.start_fetch(), "{link:?} waits for the stream");
    }
    let mut polled = ActivityJoin::new();
    polled.set_fleet_state(&POLLED);
    assert!(polled.start_fetch());
    let mut refused = ActivityJoin::new();
    refused.absorb(ActivityNews {
        refused: true,
        ..ActivityNews::default()
    });
    assert!(refused.start_fetch());
}

#[test]
fn a_missing_route_is_kept_and_not_retried_on_a_timer() {
    let mut join = ActivityJoin::new();
    join.set_fleet_state(&POLLED);
    join.start_fetch();
    join.finish_fetch(Err(Error::NotFound));
    assert_eq!(join.error(), Some(Error::NotFound));
    assert_eq!(join.fetch(), Fetch::Failed);
    assert_eq!(join.retry_in(), None, "NotFound is not retried");
    join.refresh();
    assert!(!join.start_fetch(), "a poll does not retry it either");
    assert!(join.retry());
    assert!(join.start_fetch());
    join.finish_fetch(Ok(snapshot("g1", 9)));
    assert_eq!(join.error(), None);
    assert_eq!(join.fetch(), Fetch::Idle);
}

#[test]
fn a_failed_first_snapshot_holds_the_events_and_the_next_reconciles_them() {
    let mut join = ActivityJoin::new();
    join.absorb(news(Some(1), Vec::new(), 0));
    assert!(join.start_fetch());
    join.absorb(news(Some(1), vec![item_event("g1", 8, "i4", "covered")], 1));
    join.finish_fetch(Err(Error::Busy(None)));
    assert_eq!(join.fetch(), Fetch::Failed);
    assert_eq!(join.error(), Some(Error::Busy(None)));
    assert!(join.activity().is_none());
    assert_eq!(join.retry_in(), Some(Duration::from_secs(1)));
    // Events keep arriving on the same stream and wait.
    join.absorb(news(Some(1), vec![item_event("g1", 10, "i6", "Next")], 2));
    assert_eq!(join.held(), 2);
    assert!(!join.start_fetch(), "no fetch until the retry");
    assert!(join.retry());
    assert!(join.start_fetch());
    join.finish_fetch(Ok(snapshot("g1", 9)));
    assert_eq!(join.fetch(), Fetch::Idle);
    assert_eq!(join.error(), None);
    assert_eq!(words_of(&join, "i4").as_deref(), Some("Hello"));
    assert_eq!(words_of(&join, "i6").as_deref(), Some("Next"));
    assert!(join.live(), "the same stream throughout");
}

#[test]
fn a_stale_snapshot_takes_no_event_until_a_fetch_succeeds() {
    let mut join = joined();
    join.absorb(news(Some(1), vec![item_event("g1", 12, "i4", "one")], 1));
    assert_eq!(words_of(&join, "i4").as_deref(), Some("one"));
    // `seq` fails to rise, and the fetch that follows fails.
    join.absorb(news(Some(1), vec![item_event("g1", 11, "i4", "two")], 2));
    assert!(join.start_fetch());
    join.finish_fetch(Err(Error::Busy(Some(3))));
    assert_eq!(join.fetch(), Fetch::Failed);
    assert_eq!(join.retry_in(), Some(Duration::from_secs(3)));
    // A later event of the same generation is not applied to the stale
    // snapshot, and asks for no fetch before the retry.
    join.absorb(news(Some(1), vec![item_event("g1", 13, "i4", "three")], 3));
    assert_eq!(words_of(&join, "i4").as_deref(), Some("one"));
    assert_eq!(join.fetch(), Fetch::Failed);
    assert!(!join.start_fetch());
    // Try again: the snapshot and the held events bring it back in sync.
    assert!(join.retry());
    assert!(join.start_fetch());
    join.finish_fetch(Ok(snapshot("g1", 12)));
    assert_eq!(join.fetch(), Fetch::Idle);
    assert_eq!(words_of(&join, "i4").as_deref(), Some("three"));
    join.absorb(news(Some(1), vec![item_event("g1", 14, "i4", "four")], 4));
    assert_eq!(words_of(&join, "i4").as_deref(), Some("four"));
}

#[test]
fn retries_back_off_to_30_seconds_and_honour_retry_after_up_to_it() {
    let mut join = ActivityJoin::new();
    join.set_fleet_state(&POLLED);
    let mut waits = Vec::new();
    for _ in 0..7 {
        join.retry();
        assert!(join.start_fetch(), "one fetch at a time, after each retry");
        assert!(!join.start_fetch());
        join.finish_fetch(Err(Error::Unreachable));
        waits.push(join.retry_in().unwrap().as_secs());
    }
    assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30]);
    join.retry();
    join.start_fetch();
    join.finish_fetch(Err(Error::Busy(Some(600))));
    assert_eq!(join.retry_in(), Some(Duration::from_secs(30)));
    join.retry();
    join.start_fetch();
    join.finish_fetch(Ok(snapshot("g1", 9)));
    join.refresh();
    assert!(join.start_fetch());
    join.finish_fetch(Err(Error::Unreachable));
    assert_eq!(
        join.retry_in(),
        Some(Duration::from_secs(1)),
        "a success resets it"
    );
}

#[test]
fn a_new_connection_fetches_at_once_after_a_failure() {
    let mut join = ActivityJoin::new();
    join.absorb(news(Some(1), Vec::new(), 0));
    join.start_fetch();
    join.finish_fetch(Err(Error::Unreachable));
    assert_eq!(join.fetch(), Fetch::Failed);
    join.absorb(news(Some(2), Vec::new(), 0));
    assert_eq!(join.fetch(), Fetch::Wanted);
    assert!(join.start_fetch());
}

#[test]
fn held_events_past_their_bound_are_dropped_and_fetched_again() {
    let mut join = ActivityJoin::new();
    join.absorb(news(Some(1), Vec::new(), 0));
    join.start_fetch();
    join.finish_fetch(Err(Error::Unreachable));
    let many = (1..=MAX_HELD_EVENTS as u64 + 1)
        .map(|seq| item_event("g1", seq, "i4", "x"))
        .collect();
    join.absorb(news(Some(1), many, MAX_HELD_EVENTS as u64 + 1));
    assert_eq!(join.held(), 0, "the overflow drops them all");
    join.absorb(news(
        Some(1),
        vec![item_event("g1", 900, "i4", "late")],
        600,
    ));
    assert_eq!(join.held(), 1);
    // By bytes too, while a fetch runs: it is fetched again after.
    join.retry();
    join.start_fetch();
    let mut big = item_event("g1", 901, "i4", "big");
    big.bytes = MAX_HELD_BYTES + 1;
    join.absorb(news(Some(1), vec![big], 601));
    assert_eq!(join.held(), 0);
    join.finish_fetch(Ok(snapshot("g1", 9)));
    assert_eq!(join.fetch(), Fetch::Wanted);
    assert_eq!(words_of(&join, "i4").as_deref(), Some("Hello"));
}
