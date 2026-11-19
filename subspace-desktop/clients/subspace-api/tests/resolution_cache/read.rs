use super::{cache, fill, start};
use std::time::{Duration, Instant};
use subspace_api_client::resolution_cache::{
    ResolutionCache, ResolutionLookup, ResolutionOutcome, ResolutionScope,
};

fn populated(now: Instant) -> ResolutionCache<&'static str> {
    let mut cache = cache();
    fill(&mut cache, 1..=499, Some("title"), now);
    fill(&mut cache, 500..=500, None, now);
    cache
}

#[test]
fn peeking_does_not_reorder_lru_or_start_a_read() {
    let now = Instant::now();
    let mut cache = populated(now);
    assert!(matches!(cache.peek("SUB-1", now), Some(Some(&"title"))));
    assert!(cache.peek("SUB-501", now).is_none());
    let read = start(&mut cache, "SUB-501", now);
    assert!(cache.finish(read, ResolutionOutcome::Unavailable, now));
    assert!(cache.peek("SUB-1", now).is_none());
    assert!(matches!(cache.peek("SUB-2", now), Some(Some(_))));
}

#[test]
fn peeking_skips_expired_positive_and_negative_entries_and_pending_reads() {
    let now = Instant::now();
    let mut cache = populated(now);
    let pending = start(&mut cache, "SUB-501", now);
    assert_eq!(cache.peek("SUB-500", now), Some(None));
    assert_eq!(cache.peek("SUB-501", now), None);
    assert_eq!(cache.peek("SUB-500", now + Duration::from_secs(60)), None);
    assert_eq!(cache.peek("SUB-1", now + Duration::from_secs(300)), None);
    assert!(matches!(
        cache.request("SUB-501", now),
        ResolutionLookup::Pending
    ));
    assert!(cache.finish(pending, ResolutionOutcome::Error, now));
}

#[test]
fn cards_project_live_results_without_touching_lru_or_pending_reads() {
    let now = Instant::now();
    let mut cache = populated(now);
    let pending = start(&mut cache, "SUB-501", now);
    let cards: Vec<_> = cache.cards(now).collect();
    assert_eq!(cards.len(), 500);
    assert_eq!(cards[0], ("SUB-1", Some(&"title")));
    assert_eq!(cards[499], ("SUB-500", None));
    assert_eq!(cache.cards(now + Duration::from_secs(60)).count(), 499);
    assert_eq!(cache.cards(now + Duration::from_secs(300)).count(), 0);
    assert!(matches!(
        cache.request("SUB-501", now),
        ResolutionLookup::Pending
    ));
    assert!(cache.finish(pending, ResolutionOutcome::Unavailable, now));
    assert!(cache.peek("SUB-1", now).is_none());
    assert_eq!(cache.cards(now).count(), 500);
}

#[test]
fn pending_ticket_checks_reject_replacements_completed_and_other_scopes() {
    let now = Instant::now();
    let mut cache = cache();
    let old = start(&mut cache, "SUB-1", now);
    assert!(cache.is_pending(&old));
    let next = start(&mut cache, "SUB-1", now + Duration::from_secs(60));
    assert!(!cache.is_pending(&old));
    assert!(cache.is_pending(&next));
    assert!(cache.finish(next, ResolutionOutcome::Error, now));
    assert!(!cache.is_pending(&old));
    let old = start(&mut cache, "SUB-1", now);
    cache.set_scope(ResolutionScope::new("profile", "http://127.0.0.1/api", 2));
    assert!(!cache.is_pending(&old));
}
