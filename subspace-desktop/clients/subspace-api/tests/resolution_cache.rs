use std::{
    ops::RangeInclusive,
    time::{Duration, Instant},
};

use subspace_api_client::resolution_cache::{
    ResolutionCache, ResolutionLookup, ResolutionOutcome, ResolutionRead, ResolutionScope,
};

#[path = "resolution_cache/read.rs"]
mod read;
#[path = "resolution_cache/refresh.rs"]
mod refresh;

fn cache() -> ResolutionCache<&'static str> {
    ResolutionCache::new(ResolutionScope::new("profile", "http://127.0.0.1/api", 1))
}

fn start(cache: &mut ResolutionCache<&'static str>, id: &str, now: Instant) -> ResolutionRead {
    let result = cache.request(id, now);
    assert!(matches!(result, ResolutionLookup::Start(_)));
    match result {
        ResolutionLookup::Start(read) => read,
        _ => unreachable!(),
    }
}

fn fill(
    cache: &mut ResolutionCache<&'static str>,
    numbers: RangeInclusive<usize>,
    card: Option<&'static str>,
    now: Instant,
) {
    for number in numbers {
        let read = start(cache, &format!("SUB-{number}"), now);
        let outcome = card.map_or(ResolutionOutcome::Unavailable, ResolutionOutcome::Available);
        assert!(cache.finish(read, outcome, now));
    }
}

#[test]
fn available_lifetime_expires_at_five_minutes_without_sliding() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(cache.finish(read, ResolutionOutcome::Available("title"), now));
    assert!(matches!(
        cache.request(
            "SUB-1",
            now + Duration::from_secs(300) - Duration::from_nanos(1)
        ),
        ResolutionLookup::Available(&"title")
    ));
    assert!(matches!(
        cache.request("SUB-1", now + Duration::from_secs(300)),
        ResolutionLookup::Start(_)
    ));
}

#[test]
fn unavailable_lifetime_expires_at_one_minute() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(cache.finish(read, ResolutionOutcome::Unavailable, now));
    assert!(matches!(
        cache.request(
            "SUB-1",
            now + Duration::from_secs(60) - Duration::from_nanos(1)
        ),
        ResolutionLookup::Unavailable
    ));
    assert!(matches!(
        cache.request("SUB-1", now + Duration::from_secs(60)),
        ResolutionLookup::Start(_)
    ));
}

#[test]
fn the_501st_entry_evicts_the_least_recently_read() {
    let now = Instant::now();
    let mut cache = cache();
    fill(&mut cache, 1..=500, Some("title"), now);
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Available(_)
    ));
    let read = start(&mut cache, "SUB-501", now);
    assert!(cache.finish(read, ResolutionOutcome::Unavailable, now));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Available(_)
    ));
    assert!(matches!(
        cache.request("SUB-2", now),
        ResolutionLookup::Start(_)
    ));
    assert!(matches!(
        cache.request("SUB-3", now),
        ResolutionLookup::Available(_)
    ));
    assert!(matches!(
        cache.request("SUB-501", now),
        ResolutionLookup::Unavailable
    ));
}

#[test]
fn each_scope_component_clears_cards_unavailable_and_pending() {
    let now = Instant::now();
    for scope in [
        ResolutionScope::new("other", "http://127.0.0.1/api", 1),
        ResolutionScope::new("profile", "other", 1),
        ResolutionScope::new("profile", "http://127.0.0.1/api", 2),
    ] {
        let mut cache = cache();
        let read = start(&mut cache, "SUB-1", now);
        assert!(cache.finish(read, ResolutionOutcome::Available("title"), now));
        let read = start(&mut cache, "SUB-2", now);
        assert!(cache.finish(read, ResolutionOutcome::Unavailable, now));
        let _pending = start(&mut cache, "SUB-3", now);
        cache.set_scope(scope);
        for id in ["SUB-1", "SUB-2", "SUB-3"] {
            assert!(matches!(cache.request(id, now), ResolutionLookup::Start(_)));
        }
    }
}

#[test]
fn unchanged_scope_preserves_cached_and_pending_reads() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(cache.finish(read, ResolutionOutcome::Available("title"), now));
    let _pending = start(&mut cache, "SUB-2", now);
    cache.set_scope(ResolutionScope::new("profile", "http://127.0.0.1/api", 1));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Available(_)
    ));
    assert!(matches!(
        cache.request("SUB-2", now),
        ResolutionLookup::Pending
    ));
}

#[test]
fn stale_result_cannot_replace_or_clear_a_new_pending_read() {
    let now = Instant::now();
    let mut cache = cache();
    let old = start(&mut cache, "SUB-1", now);
    assert_eq!(old.identifier(), "SUB-1");
    assert_eq!(old.generation(), 1);
    cache.set_scope(ResolutionScope::new("profile", "http://127.0.0.1/api", 2));
    let current = start(&mut cache, "SUB-1", now);
    assert!(!cache.finish(old, ResolutionOutcome::Available("old title"), now));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Pending
    ));
    assert!(cache.finish(current, ResolutionOutcome::Available("new title"), now));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Available(&"new title")
    ));
}

#[test]
fn other_errors_are_not_cached_and_can_be_retried() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(cache.finish(read, ResolutionOutcome::Error, now));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Start(_)
    ));
}

#[test]
fn duplicates_share_one_pending_read() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Pending
    ));
    assert!(matches!(
        cache.request("SUB-2", now),
        ResolutionLookup::Start(_)
    ));
    assert!(cache.finish(read, ResolutionOutcome::Available("title"), now));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Available(&"title")
    ));
}

#[test]
fn abandoned_reads_restart_at_sixty_seconds() {
    let now = Instant::now();
    let mut cache = cache();
    let _abandoned = start(&mut cache, "SUB-1", now);
    assert!(matches!(
        cache.request(
            "SUB-1",
            now + Duration::from_secs(60) - Duration::from_nanos(1)
        ),
        ResolutionLookup::Pending
    ));
    let replacement = start(&mut cache, "SUB-1", now + Duration::from_secs(60));
    assert!(matches!(
        cache.request("SUB-1", now + Duration::from_secs(60)),
        ResolutionLookup::Pending
    ));
    assert!(cache.finish(
        replacement,
        ResolutionOutcome::Available("new"),
        now + Duration::from_secs(60)
    ));
}

#[test]
fn late_abandoned_completions_cannot_end_the_replacement_read() {
    let now = Instant::now();
    for outcome in [
        ResolutionOutcome::Available("old"),
        ResolutionOutcome::Unavailable,
        ResolutionOutcome::Error,
    ] {
        let mut cache = cache();
        let old = start(&mut cache, "SUB-1", now);
        let later = now + Duration::from_secs(60);
        let replacement = start(&mut cache, "SUB-1", later);
        assert!(!cache.finish(old, outcome, later));
        assert!(matches!(
            cache.request("SUB-1", later),
            ResolutionLookup::Pending
        ));
        assert!(cache.finish(replacement, ResolutionOutcome::Available("new"), later));
        assert!(matches!(
            cache.request("SUB-1", later),
            ResolutionLookup::Available(&"new")
        ));
    }
}

#[test]
fn late_abandoned_completion_cannot_replace_a_new_cached_result() {
    let now = Instant::now();
    let mut cache = cache();
    let old = start(&mut cache, "SUB-1", now);
    let later = now + Duration::from_secs(60);
    let replacement = start(&mut cache, "SUB-1", later);
    assert!(cache.finish(replacement, ResolutionOutcome::Available("new"), later));
    assert!(!cache.finish(old, ResolutionOutcome::Unavailable, later));
    assert!(matches!(
        cache.request("SUB-1", later),
        ResolutionLookup::Available(&"new")
    ));
}

#[test]
fn stale_scope_error_leaves_the_new_pending_read_intact() {
    let now = Instant::now();
    let mut cache = cache();
    let old = start(&mut cache, "SUB-1", now);
    cache.set_scope(ResolutionScope::new("profile", "http://127.0.0.1/api", 2));
    let replacement = start(&mut cache, "SUB-1", now);
    assert!(!cache.finish(old, ResolutionOutcome::Error, now));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Pending
    ));
    assert!(cache.finish(replacement, ResolutionOutcome::Available("new"), now));
}

#[test]
fn unavailable_hits_move_to_the_most_recent_lru_position() {
    let now = Instant::now();
    let mut cache = cache();
    fill(&mut cache, 1..=500, None, now);
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Unavailable
    ));
    let read = start(&mut cache, "SUB-501", now);
    assert!(cache.finish(read, ResolutionOutcome::Available("new"), now));
    assert!(matches!(
        cache.request("SUB-1", now),
        ResolutionLookup::Unavailable
    ));
    assert!(matches!(
        cache.request("SUB-2", now),
        ResolutionLookup::Start(_)
    ));
}

#[test]
fn expired_entries_are_removed_before_evicting_a_live_lru_entry() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(cache.finish(read, ResolutionOutcome::Available("keep"), now));
    fill(&mut cache, 2..=500, None, now);
    let read = start(&mut cache, "SUB-501", now);
    let later = now + Duration::from_secs(60);
    assert!(cache.finish(read, ResolutionOutcome::Available("new"), later));
    assert!(matches!(
        cache.request("SUB-1", later),
        ResolutionLookup::Available(&"keep")
    ));
    assert!(matches!(
        cache.request("SUB-500", later),
        ResolutionLookup::Start(_)
    ));
}
