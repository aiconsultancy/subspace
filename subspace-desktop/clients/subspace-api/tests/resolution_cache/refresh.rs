//! Reading an identifier again after a write.
use std::time::Instant;

use subspace_api_client::resolution_cache::ResolutionOutcome;

use super::{cache, start};

#[test]
fn a_refresh_keeps_the_old_card_until_its_own_answer_replaces_it() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(cache.finish(read, ResolutionOutcome::Available("before"), now));
    let stale = cache.refresh("SUB-1", now);
    let fresh = cache.refresh("SUB-1", now);
    assert_eq!(cache.peek("SUB-1", now), Some(Some(&"before")));
    assert!(
        !cache.finish(stale, ResolutionOutcome::Available("stale"), now),
        "a superseded read cannot land"
    );
    assert!(cache.finish(fresh, ResolutionOutcome::Available("after"), now));
    assert_eq!(cache.peek("SUB-1", now), Some(Some(&"after")));
    assert_eq!(cache.cards(now).count(), 1, "one entry per identifier");
    let failed = cache.refresh("SUB-1", now);
    assert!(cache.finish(failed, ResolutionOutcome::Error, now));
    assert_eq!(
        cache.peek("SUB-1", now),
        Some(Some(&"after")),
        "a failed refresh keeps what was shown"
    );
}

#[test]
fn a_refresh_refused_for_sign_in_or_permission_drops_the_card() {
    let now = Instant::now();
    let mut cache = cache();
    let read = start(&mut cache, "SUB-1", now);
    assert!(cache.finish(read, ResolutionOutcome::Available("before"), now));
    let denied = cache.refresh("SUB-1", now);
    assert!(cache.finish(denied, ResolutionOutcome::Denied, now));
    assert_eq!(cache.peek("SUB-1", now), None, "no longer shown");
}
