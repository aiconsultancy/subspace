//! Events that wait for a fetch are matched to its snapshot before their
//! order is checked.

use super::*;

/// Captain's ruling on round 4, point 4: events that wait for a fetch
/// are matched to its snapshot before their order is checked, so one the
/// snapshot in flight covers never fetches again, while one it does not
/// cover still must rise past the last live `seq`.
#[test]
fn an_event_covered_by_the_snapshot_in_flight_never_fetches_again() {
    let mut join = joined();
    join.absorb(news(Some(1), vec![item_event("g1", 12, "i4", "live")], 1));
    join.refresh();
    assert!(join.start_fetch());
    // Behind the live 12, but covered by the snapshot at 14 in flight.
    join.absorb(news(Some(1), vec![item_event("g1", 11, "i4", "old")], 2));
    join.absorb(news(Some(1), vec![item_event("g1", 15, "i4", "new")], 3));
    join.finish_fetch(Ok(snapshot("g1", 14)));
    assert_eq!(join.fetch(), Fetch::Idle, "11 is covered by 14");
    assert_eq!(words_of(&join, "i4").as_deref(), Some("new"));

    // Behind the live 15 and not covered by the snapshot at 14: a gap.
    join.refresh();
    assert!(join.start_fetch());
    join.absorb(news(Some(1), vec![item_event("g1", 15, "i4", "again")], 4));
    join.finish_fetch(Ok(snapshot("g1", 14)));
    assert_eq!(join.fetch(), Fetch::Wanted);
}
