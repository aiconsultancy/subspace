//! The session's lifecycle, driven through the state machine's
//! transitions: sign-in, a new view, account changes and failures here;
//! renewal and expiry in `renewal`; the site's signal in `signal`; the
//! grant in `grant`.

use std::sync::mpsc;
use std::time::Duration;

use subspace_framework::core::Secret;

use super::grant::Now;
use super::machine::{Effect, NOT_INSTALLED, Session, Stored, Waiting};
use super::{SessionCookie, SessionFailure, SessionGrant, SessionScope};
use crate::page::SiteCookie;

mod board;
mod grant;
mod renewal;
mod signal;

const HOME: &str = "https://app.example.com/desktop/";
const WEBSITE: &str = "https://app.example.com/";
const PAGE: &str = "https://app.example.com/desktop/reports?week=12";
const ENDED: &str = "Your Desk session has ended. Unsaved changes may be lost when you retry.";
const LIFETIME: Duration = Duration::from_secs(900);

fn scope() -> SessionScope {
    SessionScope::new(HOME)
        .and_then(|scope| scope.with_signal("session/required"))
        .and_then(|scope| scope.with_reserved("session"))
        .unwrap()
}

fn cookie() -> SessionCookie {
    SessionCookie::new("__Secure-Fixture", Secret::new("opaque-fixture-value"))
}

fn grant(now: Now) -> SessionGrant {
    SessionGrant::starting(cookie(), LIFETIME, now)
}

/// A grant whose revocation reports `name` on `revoked`.
fn revocable(now: Now, revoked: &mpsc::Sender<&'static str>, name: &'static str) -> SessionGrant {
    let revoked = revoked.clone();
    grant(now).with_revoke(move || {
        let _ = revoked.send(name);
    })
}

fn revoked_within(received: &mpsc::Receiver<&'static str>) -> Option<&'static str> {
    received.recv_timeout(Duration::from_secs(5)).ok()
}

fn later(now: Now, seconds: u64) -> Now {
    let by = Duration::from_secs(seconds);
    Now {
        instant: now.instant + by,
        wall: now.wall + by,
    }
}

fn site_cookie(now: Now) -> SiteCookie {
    grant(now).site_cookie(&scope()).unwrap()
}

/// A session signed in and shown, with its first attempt under way.
fn acquiring(now: Now) -> Session {
    let mut session = Session::new(scope(), "Desk");
    assert_eq!(session.account(Some(1)), [Effect::Close]);
    assert_eq!(session.tick(now, true, None), [Effect::Acquire(1)]);
    session
}

/// A session whose view shows the home page with `grant`.
fn live(now: Now, grant: SessionGrant) -> Session {
    let mut session = acquiring(now);
    assert_eq!(
        session.acquired(1, Ok(grant), now),
        [Effect::Open(1, site_cookie(now))]
    );
    assert_eq!(
        session.installed(1, Stored::Confirmed, now, None),
        [Effect::Show(1, HOME.into())]
    );
    session
}

#[test]
fn signed_out_the_session_waits_for_an_account() {
    let now = Now::current();
    let mut session = Session::new(scope(), "Desk");
    assert_eq!(session.waiting(), Some(Waiting::SignIn));
    assert_eq!(session.account(None), []);
    assert_eq!(session.tick(now, true, None), []);
    assert_eq!(session.retry(), []);
}

#[test]
fn the_first_attempt_waits_until_the_space_is_shown() {
    let now = Now::current();
    let mut session = Session::new(scope(), "Desk");
    session.account(Some(1));
    assert_eq!(session.tick(now, false, None), []);
    assert_eq!(session.waiting(), Some(Waiting::Opening));
    assert_eq!(session.tick(now, true, None), [Effect::Acquire(1)]);
    assert_eq!(session.tick(now, true, None), [], "one attempt at a time");
}

#[test]
fn a_new_view_is_shown_only_once_the_store_confirms_the_cookie() {
    let now = Now::current();
    let mut session = acquiring(now);
    assert_eq!(
        session.acquired(1, Ok(grant(now)), now),
        [Effect::Open(1, site_cookie(now))]
    );
    assert_eq!(session.waiting(), Some(Waiting::Opening));
    assert_eq!(
        session.installed(2, Stored::Confirmed, now, None),
        [],
        "not this attempt"
    );
    assert_eq!(
        session.installed(1, Stored::Confirmed, now, None),
        [Effect::Show(1, HOME.into())]
    );
    assert_eq!(session.waiting(), None);
    assert!(!session.needs_retry());
}

#[test]
fn only_the_attempt_that_opens_a_view_may_keep_it() {
    let now = Now::current();
    let mut session = acquiring(now);
    assert!(!session.opening(1), "no view before the grant");
    session.acquired(1, Ok(grant(now)), now);
    assert!(session.opening(1));
    assert!(!session.opening(2), "not this attempt");
    assert_eq!(session.account(Some(2)), [Effect::Close]);
    assert!(
        !session.opening(1),
        "closed: a view that opens now is dropped"
    );

    let at = later(now, 780);
    let mut session = live(now, grant(now));
    session.tick(at, true, None);
    session.acquired(2, Ok(grant(at)), at);
    assert!(!session.opening(2), "a renewal installs in the view it has");
}

#[test]
fn a_new_view_the_store_refuses_or_ignores_is_dropped_and_its_grant_revoked() {
    for stored in [Stored::Refused, Stored::TimedOut] {
        let now = Now::current();
        let (sender, received) = mpsc::channel();
        let mut session = acquiring(now);
        session.acquired(1, Ok(revocable(now, &sender, "unconfirmed")), now);
        assert_eq!(
            session.installed(1, stored, now, None),
            [Effect::Discard(1)],
            "{stored:?}: no hidden view keeps the revoked grant's cookie"
        );
        assert_eq!(
            session.waiting(),
            Some(Waiting::Ended(NOT_INSTALLED.into()))
        );
        assert_eq!(revoked_within(&received), Some("unconfirmed"));
        assert!(session.needs_retry());
        assert_eq!(session.retry(), [Effect::Acquire(2)]);
    }
}

#[test]
fn sign_out_and_account_switch_close_the_view_and_revoke() {
    let now = Now::current();
    let (sender, received) = mpsc::channel();
    let mut session = live(now, revocable(now, &sender, "switched"));
    assert_eq!(session.account(Some(2)), [Effect::Close]);
    assert_eq!(revoked_within(&received), Some("switched"));
    assert_eq!(session.waiting(), Some(Waiting::Opening));
    assert_eq!(session.tick(now, true, None), [Effect::Acquire(2)]);

    session.acquired(2, Ok(revocable(now, &sender, "signed out")), now);
    session.installed(2, Stored::Confirmed, now, None);
    assert_eq!(session.account(None), [Effect::Close]);
    assert_eq!(revoked_within(&received), Some("signed out"));
    assert_eq!(session.waiting(), Some(Waiting::SignIn));
}

#[test]
fn a_grant_for_an_account_that_changed_meanwhile_is_revoked_unused() {
    let now = Now::current();
    let (sender, received) = mpsc::channel();
    let mut session = acquiring(now);
    session.account(Some(2));
    assert_eq!(
        session.acquired(1, Ok(revocable(now, &sender, "stale")), now),
        []
    );
    assert_eq!(revoked_within(&received), Some("stale"));
}

#[test]
fn a_failed_first_attempt_waits_for_retry() {
    let now = Now::current();
    let mut session = acquiring(now);
    let failure = SessionFailure::action("The fixture site refused the account.");
    assert_eq!(session.acquired(1, Err(failure), now), []);
    assert_eq!(
        session.waiting(),
        Some(Waiting::Ended(
            "The fixture site refused the account.".into()
        ))
    );
    assert_eq!(session.tick(later(now, 600), true, None), [], "only Retry");
    assert_eq!(session.retry(), [Effect::Acquire(2)]);
}
