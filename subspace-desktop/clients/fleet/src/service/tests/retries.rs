//! A Retry asked too soon waits for its fleet's interval: one waiting
//! Retry per fleet, never read right after a manual one, and never held up
//! by one asked in an older generation.

use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext};
use subspace_framework::core::Secret;

use super::ordering::{Shelf, store};
use super::{HOME, aurora, service, state};
use crate::FleetState;
use crate::service::FleetApi;

/// A service over a locked keychain whose fleet was last read at `start`.
fn locked(shelf: &Shelf, start: Instant, cx: &mut TestAppContext) -> Entity<FleetApi> {
    shelf.with(|held| held.locked = true);
    let service = service(store(shelf), aurora(HOME), cx);
    service.update(cx, |service, _| {
        service.reads.last.insert("aurora".into(), start);
    });
    service
}

/// The person's Retry at `at`, and whatever it reads.
fn retry(service: &Entity<FleetApi>, at: Instant, cx: &mut TestAppContext) {
    service.update(cx, |service, cx| service.reload_unreadable_at(at, cx));
    cx.run_until_parked();
}

/// The clock moves `seconds` on, and whatever waited for it runs.
fn later(seconds: u64, cx: &mut TestAppContext) {
    cx.executor().advance_clock(Duration::from_secs(seconds));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_waiting_retry_never_reads_again_right_after_a_manual_one(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let start = Instant::now();
    let service = locked(&shelf, start, cx);
    let before = shelf.reads("aurora");
    retry(&service, start + Duration::from_secs(3), cx);
    assert_eq!(shelf.reads("aurora"), before, "too soon: it waits");
    retry(&service, start + Duration::from_secs(5), cx);
    assert_eq!(
        shelf.reads("aurora"),
        before + 1,
        "the manual Retry at second 5"
    );
    later(2, cx);
    assert_eq!(
        shelf.reads("aurora"),
        before + 1,
        "the waiting one does not read again"
    );
}

#[gpui_kit::test]
fn a_retry_from_an_older_generation_never_blocks_one_asked_now(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let start = Instant::now();
    let service = locked(&shelf, start, cx);
    retry(&service, start + Duration::from_secs(3), cx);
    // A save the locked keychain refuses moves the fleet to a new
    // generation; its read again finds the keychain still locked.
    let saved = service.update(cx, |service, cx| {
        service.save_token("aurora", Secret::new("new"), cx)
    });
    cx.run_until_parked();
    assert!(cx.foreground_executor().block_test(saved).is_err());
    assert_eq!(state(&service, cx), FleetState::StoreUnavailable);
    let before = shelf.reads("aurora");
    retry(&service, start + Duration::from_secs(4), cx);
    later(1, cx);
    assert_eq!(
        shelf.reads("aurora"),
        before + 1,
        "the Retry asked now is read at second 5"
    );
    later(10, cx);
    assert_eq!(shelf.reads("aurora"), before + 1, "once");
}
