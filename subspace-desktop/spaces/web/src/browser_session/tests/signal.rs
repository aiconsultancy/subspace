//! The site's signal: renewing at once, returning to the page, the guard
//! against a renewal that did not help, and a signal that waits for a
//! failed renewal's retry.

use super::super::SessionFailure;
use super::super::grant::Now;
use super::super::machine::{Effect, Stored, Waiting};
use super::{ENDED, HOME, PAGE, grant, later, live, site_cookie};

#[test]
fn a_signal_after_a_failed_renewal_waits_for_the_retry() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let at = later(now, 780);
    session.tick(at, true, None);
    let failure = SessionFailure::temporary("The fixture site is unreachable.");
    session.acquired(2, Err(failure), at);
    assert_eq!(
        session.signal(later(at, 1), None),
        [],
        "the retry is 30 seconds away"
    );
    let retried = later(at, 30);
    assert_eq!(session.tick(retried, true, None), [Effect::Acquire(3)]);
    session.acquired(3, Ok(grant(retried)), retried);
    assert_eq!(
        session.installed(3, Stored::Confirmed, retried, Some(PAGE.into())),
        [Effect::Return(PAGE.into())],
        "the signal waited for the retry, so the page returns"
    );
    assert_eq!(
        session.signal(later(retried, 5), None),
        [Effect::Close],
        "and the guard holds"
    );
}

#[test]
fn a_signalled_renewal_that_fails_returns_to_the_page_once_its_retry_succeeds() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let signalled = later(now, 60);
    assert_eq!(session.signal(signalled, None), [Effect::Acquire(2)]);
    let failure = SessionFailure::temporary("The fixture site is unreachable.");
    assert_eq!(
        session.acquired(2, Err(failure.clone()), signalled),
        [Effect::Notice(failure.message)]
    );
    let retried = later(signalled, 30);
    assert_eq!(session.tick(retried, true, None), [Effect::Acquire(3)]);
    session.acquired(3, Ok(grant(retried)), retried);
    assert_eq!(
        session.installed(3, Stored::Confirmed, retried, Some(PAGE.into())),
        [Effect::Return(PAGE.into())]
    );

    let renew_at = later(retried, 780);
    assert_eq!(session.tick(renew_at, true, None), [Effect::Acquire(4)]);
    session.acquired(4, Ok(grant(renew_at)), renew_at);
    assert_eq!(
        session.installed(4, Stored::Confirmed, renew_at, Some(PAGE.into())),
        [Effect::Renewed],
        "the signal was answered: the next renewal leaves the page alone"
    );
}

#[test]
fn a_signal_after_renewing_failed_for_good_ends_the_session() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let at = later(now, 780);
    session.tick(at, true, None);
    session.acquired(2, Err(SessionFailure::action("Sign in again.")), at);
    assert!(session.needs_retry());
    assert_eq!(session.signal(later(at, 1), None), [Effect::Close]);
    assert_eq!(session.waiting(), Some(Waiting::Ended(ENDED.into())));
}

#[test]
fn the_signal_renews_at_once_and_returns_to_the_page() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let signalled = later(now, 60);
    assert_eq!(session.signal(signalled, None), [Effect::Acquire(2)]);
    assert_eq!(session.signal(signalled, None), [], "one renewal at a time");
    assert_eq!(
        session.acquired(2, Ok(grant(signalled)), signalled),
        [Effect::Install(2, site_cookie(signalled))]
    );
    assert_eq!(
        session.installed(2, Stored::Confirmed, signalled, Some(PAGE.into())),
        [Effect::Return(PAGE.into())]
    );
    assert_eq!(
        session.signal(later(signalled, 5), None),
        [Effect::Close],
        "renewing did not help"
    );
    assert_eq!(session.waiting(), Some(Waiting::Ended(ENDED.into())));
}

#[test]
fn a_later_signal_renews_again() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    session.signal(now, None);
    session.acquired(2, Ok(grant(now)), now);
    session.installed(2, Stored::Confirmed, now, None);
    let much_later = later(now, 31);
    assert_eq!(session.signal(much_later, None), [Effect::Acquire(3)]);
}

#[test]
fn a_signal_during_a_renewal_returns_to_the_page_once_it_is_installed() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let renew_at = later(now, 780);
    session.tick(renew_at, true, None);
    assert_eq!(session.signal(renew_at, None), []);
    session.acquired(2, Ok(grant(renew_at)), renew_at);
    assert_eq!(session.signal(renew_at, None), []);
    assert_eq!(
        session.installed(2, Stored::Confirmed, renew_at, None),
        [Effect::Return(HOME.into())],
        "a view outside the scope returns home"
    );
}
