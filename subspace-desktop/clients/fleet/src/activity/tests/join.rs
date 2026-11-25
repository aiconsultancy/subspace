//! The snapshot joined with the officer's events: no gap, no duplicate,
//! and a fetch again whenever the events cannot be trusted to follow on.

use super::*;
use crate::activity::Fetch;

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
    join.set_polling(true);
    join.start_fetch();
    join.set_polling(false);
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
    let mut polled = ActivityJoin::new();
    polled.set_polling(true);
    assert!(polled.start_fetch());
    let mut refused = ActivityJoin::new();
    refused.absorb(ActivityNews {
        refused: true,
        ..ActivityNews::default()
    });
    assert!(refused.start_fetch());
}

#[test]
fn a_failed_fetch_is_kept_until_one_succeeds() {
    let mut join = ActivityJoin::new();
    join.set_polling(true);
    join.start_fetch();
    join.finish_fetch(Err(Error::NotFound));
    assert_eq!(join.error(), Some(Error::NotFound));
    assert_eq!(join.fetch(), Fetch::Idle);
    join.refresh();
    join.start_fetch();
    join.finish_fetch(Ok(snapshot("g1", 9)));
    assert_eq!(join.error(), None);
}
