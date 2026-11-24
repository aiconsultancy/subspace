//! The grant: its cookie's checks, its lifetime and its formatting.

use std::time::{Duration, SystemTime};

use subspace_framework::core::Secret;

use super::super::grant::Now;
use super::super::machine::{Effect, Waiting};
use super::super::{MAX_SESSION_LIFETIME, SessionCookie, SessionGrant};
use super::{LIFETIME, acquiring, cookie, grant, later, scope, site_cookie};

#[test]
fn a_product_mistake_is_logged_and_a_site_mistake_is_not() {
    let now = Now::current();
    let mut session = acquiring(now);
    let wide = SessionGrant::starting(cookie().with_path("/"), LIFETIME, now);
    let effects = session.acquired(1, Ok(wide), now);
    assert_eq!(
        effects,
        [Effect::Log(
            "The session cookie's path is outside the session's scope."
        )]
    );
    assert!(
        matches!(session.waiting(), Some(Waiting::Ended(message)) if message.contains("session settings"))
    );

    let mut session = acquiring(now);
    let bad_value = SessionCookie::new("__Secure-Fixture", Secret::new("a;b"));
    let effects = session.acquired(1, Ok(SessionGrant::starting(bad_value, LIFETIME, now)), now);
    assert_eq!(effects, []);
    assert!(
        matches!(session.waiting(), Some(Waiting::Ended(message)) if message.contains("cannot use"))
    );
}

#[test]
fn an_expired_grant_is_refused() {
    let now = Now::current();
    let mut session = acquiring(now);
    let spent = SessionGrant::starting(cookie(), LIFETIME, now);
    assert_eq!(session.acquired(1, Ok(spent), later(now, 900)), []);
    assert!(
        matches!(session.waiting(), Some(Waiting::Ended(message)) if message.contains("expired"))
    );
}

#[test]
fn a_grant_lasts_at_most_the_maximum_and_never_panics() {
    let now = Now::current();
    let forever = SessionGrant::starting(cookie(), Duration::MAX, now);
    assert!(
        !forever
            .ends
            .passed(later(now, MAX_SESSION_LIFETIME.as_secs() - 1))
    );
    assert!(
        forever
            .ends
            .passed(later(now, MAX_SESSION_LIFETIME.as_secs()))
    );
    let cookie = forever.site_cookie(&scope()).unwrap();
    assert_eq!(cookie.expires, now.wall + MAX_SESSION_LIFETIME);
}

#[test]
fn the_cookie_is_host_only_and_inside_the_scope() {
    let now = Now::current();
    let cookie = site_cookie(now);
    assert_eq!(cookie.domain, "app.example.com");
    assert_eq!(cookie.path, "/desktop/");
    assert_eq!(cookie.value.expose(), "opaque-fixture-value");
    assert_eq!(cookie.expires, now.wall + LIFETIME);
    let narrower =
        SessionGrant::starting(self::cookie().with_path("/desktop/reports"), LIFETIME, now);
    assert_eq!(
        narrower.site_cookie(&scope()).unwrap().path,
        "/desktop/reports"
    );
}

#[test]
fn a_cookie_needs_a_token_name_and_an_opaque_value() {
    let now = Now::current();
    for (name, value) in [
        ("", "value"),
        ("a name", "value"),
        ("a;b", "value"),
        ("name", ""),
        ("name", "a b"),
        ("name", "a;b"),
        ("name", "a,b"),
        ("name", "\"quoted\""),
        ("name", "a\\b"),
    ] {
        let invalid =
            SessionGrant::starting(SessionCookie::new(name, Secret::new(value)), LIFETIME, now);
        assert!(invalid.site_cookie(&scope()).is_err(), "{name}={value}");
    }
    let long = SessionCookie::new("name", Secret::new("v".repeat(4096)));
    assert!(
        SessionGrant::starting(long, LIFETIME, now)
            .site_cookie(&scope())
            .is_err()
    );
}

#[test]
fn formatting_shows_the_cookie_name_but_never_its_value() {
    assert_eq!(
        format!("{:?}", cookie()),
        "SessionCookie { name: \"__Secure-Fixture\", value: ***, path: None }"
    );
    assert_eq!(
        format!("{:?}", grant(Now::current())),
        "SessionGrant { cookie: SessionCookie { name: \"__Secure-Fixture\", value: ***, path: None }, revoke: false, .. }"
    );
}

#[test]
fn now_moves_both_clocks() {
    let now = Now::current();
    let deadline = now.after(Duration::from_secs(10));
    assert!(!deadline.passed(now));
    assert!(deadline.passed(later(now, 10)));
    assert!(deadline.passed(Now {
        instant: now.instant,
        wall: SystemTime::now() + Duration::from_secs(11),
    }));
}
