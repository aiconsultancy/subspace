use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{Activity, Late, expire, expired, late, mark_alive, observe, settle, stalled, update};
use crate::ChatId;
use crate::failure::TurnContext;

const WAIT: Duration = Duration::from_secs(90);

fn activity(busy: bool, last_event: Instant, generation: u64) -> Activity {
    Activity {
        busy,
        last_event,
        turn: TurnContext::default(),
        generation,
        expired: None,
    }
}

/// Chat 1 on generation 4, silent for longer than the wait.
fn silent(now: Instant) -> HashMap<ChatId, Activity> {
    HashMap::from([(1, activity(true, now - WAIT, 4))])
}

#[test]
fn only_a_busy_chat_silent_for_the_whole_wait_is_stalled() {
    let last = Instant::now();
    let later = last + WAIT;
    assert!(stalled(later, &activity(true, last, 1), WAIT));
    assert!(!stalled(later, &activity(false, last, 1), WAIT));
    assert!(!stalled(
        later - Duration::from_secs(1),
        &activity(true, last, 1),
        WAIT
    ));
}

#[test]
fn a_ping_without_an_answer_ends_the_turn_it_saw_and_records_it() {
    let now = Instant::now();
    let mut chats = silent(now);
    chats.insert(2, activity(true, now, 1));
    let observed = observe(&chats, now, WAIT);
    assert_eq!(observed, [(1, 4)]);
    let ended = expire(&mut chats, &observed, now, WAIT);
    assert_eq!(ended.len(), 1);
    assert!(!chats[&1].busy);
    assert_eq!(chats[&1].expired, Some(4));
    assert!(chats[&2].busy, "a chat with a fresh event keeps its turn");
}

/// (a) An event arrives while the ping runs: the turn is not ended.
#[test]
fn an_event_during_the_ping_keeps_the_turn() {
    let now = Instant::now();
    let mut chats = silent(now);
    let observed = observe(&chats, now, WAIT);
    update(&mut chats, 1, activity(true, now, 4));
    assert!(expire(&mut chats, &observed, now, WAIT).is_empty());
    assert!(chats[&1].busy);
}

/// (b) A new turn starts while the ping runs: the new turn is not ended,
/// even if its first event is old enough to look silent.
#[test]
fn a_new_turn_during_the_ping_is_not_ended() {
    let now = Instant::now();
    let mut chats = silent(now);
    let observed = observe(&chats, now, WAIT);
    update(&mut chats, 1, activity(true, now - WAIT, 5));
    assert!(expire(&mut chats, &observed, now, WAIT).is_empty());
    assert!(chats[&1].busy);
    assert_eq!(chats[&1].expired, None);
}

/// (c) The watchdog ended generation 4. The actor's later update for it
/// stays ended, the actor may not report its end again, and the turn's own
/// idle or error is ignored.
#[test]
fn a_turn_the_watchdog_ended_is_not_ended_again() {
    let now = Instant::now();
    let mut chats = silent(now);
    let observed = observe(&chats, now, WAIT);
    assert_eq!(expire(&mut chats, &observed, now, WAIT).len(), 1);
    update(&mut chats, 1, activity(true, now, 4));
    assert!(!chats[&1].busy, "an update cannot revive an expired turn");
    assert!(expired(&chats, 1, 4));
    assert!(!settle(&mut chats, 1, 4), "no second TurnDone or failure");
    assert_eq!(late(true, "session.idle"), Late::Ignore);
    assert_eq!(late(true, "session.error"), Late::Ignore);
    assert_eq!(late(false, "session.idle"), Late::Normal);
}

/// The other way round: the actor ended generation 4 first, so the
/// watchdog leaves it alone.
#[test]
fn the_watchdog_does_not_end_a_turn_the_actor_ended() {
    let now = Instant::now();
    let mut chats = silent(now);
    let observed = observe(&chats, now, WAIT);
    assert!(settle(&mut chats, 1, 4));
    assert!(expire(&mut chats, &observed, now, WAIT).is_empty());
    assert_eq!(chats[&1].expired, None);
}

#[test]
fn an_older_generation_changes_nothing() {
    let now = Instant::now();
    let mut chats = HashMap::from([(1, activity(true, now, 6))]);
    update(&mut chats, 1, activity(false, now, 5));
    assert!(chats[&1].busy);
    assert_eq!(chats[&1].generation, 6);
}

#[test]
fn an_answered_ping_restarts_the_wait_of_the_same_turn_only() {
    let now = Instant::now();
    let mut chats = silent(now);
    chats.insert(2, activity(true, now - WAIT, 2));
    let observed = observe(&chats, now, WAIT);
    update(&mut chats, 2, activity(true, now - WAIT, 3));
    mark_alive(&mut chats, &observed, now);
    assert_eq!(chats[&1].last_event, now);
    assert_eq!(
        chats[&2].last_event,
        now - WAIT,
        "a new turn keeps its own time"
    );
}

/// A late reply after the watchdog ended the turn means the runtime
/// recovered: the turn goes on as a new generation.
#[test]
fn late_output_rearms_the_turn() {
    for kind in [
        "assistant.message_delta",
        "assistant.message",
        "tool.execution_start",
    ] {
        assert_eq!(late(true, kind), Late::Rearm, "{kind}");
        assert_eq!(late(false, kind), Late::Normal, "{kind}");
    }
    assert_eq!(late(true, "session.title_changed"), Late::Normal);
}
