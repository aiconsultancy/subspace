//! A start that moves around the site's signal: while the site refuses the
//! session, when the site sends the move's own load to its signal, after
//! renewing failed for good, once the move's load lands, and in a later
//! session.

use super::super::SessionFailure;
use super::super::grant::Now;
use super::super::machine::{Effect, Stored};
use super::start::{BLUE, ELSEWHERE, GREEN, page};
use super::{PAGE, grant, later, live};

#[test]
fn the_signal_returns_to_the_start_when_the_view_has_no_page() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    session.route(Some("teams/green"), page());
    assert_eq!(session.signal(now, None), [Effect::Acquire(2)]);
    session.acquired(2, Ok(grant(now)), now);
    assert_eq!(
        session.installed(2, Stored::Confirmed, now, None),
        [Effect::Load(GREEN.into())]
    );
}

#[test]
fn a_start_moved_while_the_site_refuses_the_session_opens_once_renewed() {
    for install_first in [false, true] {
        let now = Now::current();
        let mut session = live(now, grant(now));
        assert_eq!(session.signal(now, Some(PAGE.into())), [Effect::Acquire(2)]);
        if install_first {
            session.acquired(2, Ok(grant(now)), now);
        }
        assert_eq!(
            session.route(Some("?team=blue"), page()),
            [],
            "the site would send a load now back to the signal"
        );
        if !install_first {
            session.acquired(2, Ok(grant(now)), now);
        }
        assert_eq!(
            session.installed(2, Stored::Confirmed, now, Some(PAGE.into())),
            [Effect::Load(BLUE.into())],
            "not back to the page from before the move"
        );
        session.signal(later(now, 31), Some(PAGE.into()));
        session.acquired(3, Ok(grant(now)), now);
        assert_eq!(
            session.installed(3, Stored::Confirmed, now, Some(PAGE.into())),
            [Effect::Load(PAGE.into())],
            "a later signal returns to the page the view shows"
        );
    }
}

#[test]
fn a_start_moved_while_the_signal_waits_for_a_failed_renewal_opens_once_renewed() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let renew_at = later(now, 780);
    session.tick(renew_at, true, Some(PAGE.into()));
    let failure = SessionFailure::temporary("The fixture site is busy.");
    session.acquired(2, Err(failure), renew_at);
    assert_eq!(session.signal(renew_at, Some(PAGE.into())), []);
    assert_eq!(session.route(Some("?team=blue"), page()), []);
    let retry_at = later(renew_at, 30);
    assert_eq!(
        session.tick(retry_at, true, Some(PAGE.into())),
        [Effect::Acquire(3)]
    );
    session.acquired(3, Ok(grant(retry_at)), retry_at);
    assert_eq!(
        session.installed(3, Stored::Confirmed, retry_at, Some(PAGE.into())),
        [Effect::Load(BLUE.into())]
    );
}

#[test]
fn after_renewing_failed_for_good_retry_opens_a_moved_start() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let renew_at = later(now, 780);
    session.tick(renew_at, true, Some(PAGE.into()));
    let failure = SessionFailure::action("The fixture site refused the account.");
    session.acquired(2, Err(failure), renew_at);
    assert!(session.needs_retry());
    assert_eq!(
        session.route(Some("?team=blue"), page()),
        [Effect::Load(BLUE.into())],
        "the cookie still works until it expires"
    );
    assert_eq!(
        session.signal(renew_at, Some(PAGE.into())),
        [Effect::Close],
        "the site sent the load back to the signal"
    );
    assert_eq!(session.retry(), [Effect::Acquire(3)]);
    session.acquired(3, Ok(grant(renew_at)), renew_at);
    assert_eq!(
        session.installed(3, Stored::Confirmed, renew_at, None),
        [Effect::Show(3, BLUE.into())]
    );
}

#[test]
fn a_new_session_after_a_move_returns_to_its_own_page() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    session.signal(now, Some(PAGE.into()));
    session.route(Some("?team=blue"), page());
    assert_eq!(session.account(Some(2)), [Effect::Close]);
    session.tick(now, true, None);
    session.acquired(3, Ok(grant(now)), now);
    assert_eq!(
        session.installed(3, Stored::Confirmed, now, None),
        [Effect::Show(3, BLUE.into())]
    );
    assert_eq!(session.signal(now, Some(PAGE.into())), [Effect::Acquire(4)]);
    session.acquired(4, Ok(grant(now)), now);
    assert_eq!(
        session.installed(4, Stored::Confirmed, now, Some(PAGE.into())),
        [Effect::Load(PAGE.into())],
        "the start did not move during this session"
    );
}

#[test]
fn a_move_the_site_sends_back_to_its_signal_opens_once_renewed() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    assert_eq!(
        session.route(Some("?team=blue"), page()),
        [Effect::Load(BLUE.into())]
    );
    // The site has dropped the session: it sends the load to the signal,
    // which the view refuses, so the view stays on the page the move left.
    assert_eq!(session.signal(now, page()), [Effect::Acquire(2)]);
    session.acquired(2, Ok(grant(now)), now);
    assert_eq!(
        session.installed(2, Stored::Confirmed, now, page()),
        [Effect::Load(BLUE.into())]
    );
}

#[test]
fn a_move_sent_back_to_the_signal_within_its_guard_retries_at_the_start() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    session.signal(now, page());
    session.acquired(2, Ok(grant(now)), now);
    assert_eq!(
        session.installed(2, Stored::Confirmed, now, page()),
        [Effect::Load(PAGE.into())]
    );
    session.route(Some("?team=blue"), page());
    assert_eq!(
        session.signal(later(now, 5), page()),
        [Effect::Close],
        "a second signal within 30 seconds"
    );
    assert_eq!(session.retry(), [Effect::Acquire(3)]);
    session.acquired(3, Ok(grant(now)), now);
    assert_eq!(
        session.installed(3, Stored::Confirmed, now, None),
        [Effect::Show(3, BLUE.into())]
    );
}

#[test]
fn once_a_move_lands_the_view_returns_and_resumes_where_it_is() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    session.route(Some("?team=blue"), page());
    assert_eq!(
        session.signal(now, Some(ELSEWHERE.into())),
        [Effect::Acquire(2)]
    );
    session.acquired(2, Ok(grant(now)), now);
    assert_eq!(
        session.installed(2, Stored::Confirmed, now, Some(ELSEWHERE.into())),
        [Effect::Load(ELSEWHERE.into())]
    );

    let mut session = live(now, grant(now));
    let renew_at = later(now, 780);
    session.tick(renew_at, true, page());
    let failure = SessionFailure::action("The fixture site refused the account.");
    session.acquired(2, Err(failure), renew_at);
    session.route(Some("?team=blue"), page());
    assert_eq!(
        session.tick(later(now, 901), true, Some(ELSEWHERE.into())),
        [Effect::Close]
    );
    assert_eq!(session.retry(), [Effect::Acquire(3)]);
    session.acquired(3, Ok(grant(renew_at)), renew_at);
    assert_eq!(
        session.installed(3, Stored::Confirmed, renew_at, None),
        [Effect::Show(3, ELSEWHERE.into())],
        "the person moved on after the start's load landed"
    );
}
