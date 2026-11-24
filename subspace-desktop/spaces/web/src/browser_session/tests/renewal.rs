//! Renewal and expiry, and where the view resumes after a session ends.

use std::sync::mpsc;
use std::time::Duration;

use super::super::grant::Now;
use super::super::machine::{Effect, INVALID, NOT_INSTALLED, Stored, Waiting};
use super::super::{MIN_SESSION_LIFETIME, SessionFailure, SessionGrant};
use super::{
    ENDED, HOME, PAGE, acquiring, cookie, grant, later, live, revocable, revoked_within,
    site_cookie,
};

#[test]
fn a_session_renews_before_it_ends_and_revokes_the_grant_it_replaces() {
    let now = Now::current();
    let (sender, received) = mpsc::channel();
    let mut session = live(now, revocable(now, &sender, "first"));
    assert_eq!(session.tick(later(now, 779), true, None), []);
    let renew_at = later(now, 780);
    assert_eq!(session.tick(renew_at, true, None), [Effect::Acquire(2)]);
    assert_eq!(session.waiting(), None, "the view keeps working meanwhile");
    assert_eq!(
        session.acquired(2, Ok(revocable(renew_at, &sender, "second")), renew_at),
        [Effect::Install(2, site_cookie(renew_at))]
    );
    assert!(received.try_recv().is_err(), "not until the store confirms");
    assert_eq!(
        session.installed(2, Stored::Confirmed, renew_at, None),
        [Effect::Renewed]
    );
    assert_eq!(revoked_within(&received), Some("first"));
    assert!(received.recv_timeout(Duration::from_millis(100)).is_err());
}

#[test]
fn the_shortest_grant_renews_halfway_and_a_shorter_one_is_refused() {
    let now = Now::current();
    let mut session = acquiring(now);
    let shortest = SessionGrant::starting(cookie(), MIN_SESSION_LIFETIME, now);
    session.acquired(1, Ok(shortest), now);
    session.installed(1, Stored::Confirmed, now, None);
    assert_eq!(session.tick(later(now, 119), true, None), []);
    assert_eq!(
        session.tick(later(now, 120), true, None),
        [Effect::Acquire(2)]
    );

    let at = later(now, 120);
    let short = SessionGrant::starting(cookie(), MIN_SESSION_LIFETIME - Duration::from_secs(1), at);
    assert_eq!(
        session.acquired(2, Ok(short), at),
        [Effect::Notice(INVALID.into())],
        "a renewal every second would never stop"
    );
    assert!(
        session.waiting().is_none(),
        "the session it renews still holds"
    );
}

#[test]
fn a_renewal_the_store_does_not_confirm_in_time_ends_the_session() {
    let now = Now::current();
    let (sender, received) = mpsc::channel();
    let mut session = live(now, revocable(now, &sender, "live"));
    let renew_at = later(now, 780);
    session.tick(renew_at, true, None);
    session.acquired(2, Ok(revocable(renew_at, &sender, "unconfirmed")), renew_at);
    assert_eq!(
        session.installed(2, Stored::TimedOut, renew_at, None),
        [Effect::Close],
        "a late write could replace the live cookie with a revoked one"
    );
    let mut revoked = [revoked_within(&received), revoked_within(&received)];
    revoked.sort_unstable();
    assert_eq!(revoked, [Some("live"), Some("unconfirmed")]);
    assert_eq!(
        session.waiting(),
        Some(Waiting::Ended(NOT_INSTALLED.into()))
    );
}

#[test]
fn a_renewal_the_store_refuses_ends_the_session() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let renew_at = later(now, 780);
    session.tick(renew_at, true, None);
    session.acquired(2, Ok(grant(renew_at)), renew_at);
    assert_eq!(
        session.installed(2, Stored::Refused, renew_at, None),
        [Effect::Close]
    );
    assert_eq!(
        session.waiting(),
        Some(Waiting::Ended(NOT_INSTALLED.into()))
    );
}

#[test]
fn a_passing_renewal_failure_retries_three_times_then_waits_for_retry() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let mut at = later(now, 780);
    let failure = SessionFailure::temporary("The fixture site is unreachable.");
    for attempt in 2..=4 {
        assert_eq!(session.tick(at, true, None), [Effect::Acquire(attempt)]);
        session.acquired(attempt, Err(failure.clone()), at);
        assert!(!session.needs_retry());
        at = later(at, 30);
    }
    assert_eq!(session.tick(at, true, None), [Effect::Acquire(5)]);
    assert_eq!(
        session.acquired(5, Err(failure.clone()), at),
        [Effect::Notice(failure.message.clone())]
    );
    assert_eq!(session.tick(later(at, 10), true, None), []);
    assert!(session.needs_retry(), "Reload means Retry now");
    assert_eq!(session.retry(), [Effect::Acquire(6)]);
}

#[test]
fn expiry_ends_the_session_and_retry_reopens_where_the_view_was() {
    let now = Now::current();
    let (sender, received) = mpsc::channel();
    let mut session = live(now, revocable(now, &sender, "expired"));
    let ended = later(now, 900);
    assert_eq!(
        session.tick(ended, true, Some(PAGE.into())),
        [Effect::Close]
    );
    assert_eq!(revoked_within(&received), Some("expired"));
    assert_eq!(session.waiting(), Some(Waiting::Ended(ENDED.into())));
    assert_eq!(session.retry(), [Effect::Acquire(2)]);
    session.acquired(2, Ok(grant(ended)), ended);
    assert_eq!(
        session.installed(2, Stored::Confirmed, ended, None),
        [Effect::Show(2, PAGE.into())]
    );
}

#[test]
fn a_session_the_signal_or_the_store_ends_resumes_where_the_view_was() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    session.signal(now, None);
    session.acquired(2, Ok(grant(now)), now);
    session.installed(2, Stored::Confirmed, now, None);
    assert_eq!(
        session.signal(later(now, 5), Some(PAGE.into())),
        [Effect::Close]
    );
    session.retry();
    session.acquired(3, Ok(grant(now)), now);
    assert_eq!(
        session.installed(3, Stored::Confirmed, now, None),
        [Effect::Show(3, PAGE.into())]
    );

    let renew_at = later(now, 780);
    session.tick(renew_at, true, None);
    session.acquired(4, Ok(grant(renew_at)), renew_at);
    let elsewhere = "https://app.example.com/desktop/inbox";
    assert_eq!(
        session.installed(4, Stored::Refused, renew_at, Some(elsewhere.into())),
        [Effect::Close]
    );
    session.retry();
    session.acquired(5, Ok(grant(renew_at)), renew_at);
    assert_eq!(
        session.installed(5, Stored::Confirmed, renew_at, None),
        [Effect::Show(5, elsewhere.into())]
    );
}

#[test]
fn another_account_starts_at_the_home_page() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    assert_eq!(session.account(Some(2)), [Effect::Close]);
    assert_eq!(
        session.tick(now, true, Some(PAGE.into())),
        [Effect::Acquire(2)]
    );
    session.acquired(2, Ok(grant(now)), now);
    assert_eq!(
        session.installed(2, Stored::Confirmed, now, Some(PAGE.into())),
        [Effect::Show(2, HOME.into())],
        "the last account's page is not this one's"
    );
}

#[test]
fn a_session_ends_when_the_wall_clock_passes_its_end_during_sleep() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let woke = Now {
        instant: now.instant + Duration::from_secs(1),
        wall: now.wall + Duration::from_secs(901),
    };
    assert_eq!(session.tick(woke, true, None), [Effect::Close]);
}

#[test]
fn a_session_ends_when_it_expires_while_a_renewal_is_under_way() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    session.tick(later(now, 780), true, None);
    assert_eq!(session.tick(later(now, 900), true, None), [Effect::Close]);
    assert_eq!(
        session.acquired(2, Ok(grant(now)), now),
        [],
        "the renewal no longer applies"
    );
}
