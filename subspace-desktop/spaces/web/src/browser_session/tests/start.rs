//! The product's start route: where a new view starts, where a view
//! showing a page goes when the start moves, Retry and a new view after
//! the start moved, another account in the same poll, a route outside the
//! scope, and Go to the start page in a test window. `moved` covers moves
//! around the site's signal.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::super::grant::Now;
use super::super::machine::{Effect, OUTSIDE, Session, Stored};
use super::board::{refusing, shown};
use super::{HOME, PAGE, acquiring, grant, later, live, scope, site_cookie};
use crate::WebBoard;

pub(super) const BLUE: &str = "https://app.example.com/desktop/?team=blue";
pub(super) const ELSEWHERE: &str = "https://app.example.com/desktop/reports?week=13";
pub(super) const GREEN: &str = "https://app.example.com/desktop/teams/green";

/// Where the view is when the start moves: the page it shows.
pub(super) fn page() -> Option<String> {
    Some(PAGE.into())
}

#[test]
fn a_new_view_starts_where_the_product_routes_it() {
    let now = Now::current();
    let mut session = Session::new(scope(), "Desk");
    assert_eq!(session.start(), HOME);
    assert_eq!(session.route(Some("?team=blue"), page()), [], "no view yet");
    assert_eq!(session.start(), BLUE);
    session.account(Some(1));
    assert_eq!(session.tick(now, true, None), [Effect::Acquire(1)]);
    session.acquired(1, Ok(grant(now)), now);
    assert_eq!(
        session.installed(1, Stored::Confirmed, now, None),
        [Effect::Show(1, BLUE.into())]
    );
}

#[test]
fn a_view_showing_a_page_goes_to_a_moved_start_at_once() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    assert_eq!(
        session.route(Some("?team=blue"), page()),
        [Effect::Load(BLUE.into())]
    );
    assert_eq!(
        session.route(Some("?team=blue"), page()),
        [],
        "the same start"
    );
    assert_eq!(
        session.route(Some("teams/green"), page()),
        [Effect::Load(GREEN.into())]
    );
    assert_eq!(session.route(None, page()), [Effect::Load(HOME.into())]);
    assert_eq!(session.route(Some(""), page()), [], "the home again");
}

#[test]
fn a_moved_start_reaches_a_view_whose_session_is_renewing() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let renew_at = later(now, 780);
    assert_eq!(session.tick(renew_at, true, None), [Effect::Acquire(2)]);
    assert_eq!(
        session.route(Some("?team=blue"), page()),
        [Effect::Load(BLUE.into())]
    );
    session.acquired(2, Ok(grant(renew_at)), renew_at);
    assert_eq!(
        session.route(Some("teams/green"), page()),
        [Effect::Load(GREEN.into())]
    );
    assert_eq!(
        session.installed(2, Stored::Confirmed, renew_at, Some(GREEN.into())),
        [Effect::Renewed]
    );
    assert_eq!(
        session.signal(renew_at, page()),
        [Effect::Acquire(3)],
        "the person went back to the page the move left"
    );
    session.acquired(3, Ok(grant(renew_at)), renew_at);
    assert_eq!(
        session.installed(3, Stored::Confirmed, renew_at, page()),
        [Effect::Load(PAGE.into())],
        "the renewal forgot the page the move left"
    );
}

#[test]
fn retry_after_the_start_moved_opens_the_start_not_where_the_view_was() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let ended = later(now, 901);
    // The board's order in one poll: the clock, then the route.
    assert_eq!(
        session.tick(ended, true, Some(PAGE.into())),
        [Effect::Close]
    );
    assert_eq!(
        session.route(Some("?team=blue"), page()),
        [],
        "no view to move"
    );
    assert_eq!(session.retry(), [Effect::Acquire(2)]);
    session.acquired(2, Ok(grant(ended)), ended);
    assert_eq!(
        session.installed(2, Stored::Confirmed, ended, None),
        [Effect::Show(2, BLUE.into())]
    );
}

#[test]
fn a_new_view_under_way_when_the_start_moves_opens_the_start() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    let ended = later(now, 901);
    session.tick(ended, true, Some(PAGE.into()));
    assert_eq!(session.retry(), [Effect::Acquire(2)]);
    assert_eq!(session.route(Some("teams/green"), page()), []);
    assert_eq!(
        session.acquired(2, Ok(grant(ended)), ended),
        [Effect::Open(2, site_cookie(ended))]
    );
    assert_eq!(
        session.route(Some("?team=blue"), page()),
        [],
        "still no page"
    );
    assert_eq!(
        session.installed(2, Stored::Confirmed, ended, None),
        [Effect::Show(2, BLUE.into())]
    );
}

#[test]
fn a_route_outside_the_scope_is_logged_and_starts_at_the_home() {
    let mut session = acquiring(Now::current());
    assert_eq!(
        session.route(Some("../other/"), page()),
        [Effect::Log(OUTSIDE)]
    );
    assert_eq!(session.start(), HOME);

    let now = Now::current();
    let mut session = live(now, grant(now));
    session.route(Some("?team=blue"), page());
    assert_eq!(
        session.route(Some("session"), page()),
        [Effect::Log(OUTSIDE), Effect::Load(HOME.into())]
    );
    for refused in ["session/required", "//elsewhere.example.com/desktop/"] {
        assert_eq!(
            session.route(Some(refused), page()),
            [Effect::Log(OUTSIDE)],
            "{refused}: still the home"
        );
        assert_eq!(session.start(), HOME, "{refused}");
    }
}

#[gpui_kit::test]
fn go_to_the_start_page_follows_the_products_route(cx: &mut TestAppContext) {
    let signed_in = Rc::new(Cell::new(1));
    let attempts = Arc::new(AtomicUsize::new(0));
    let route = Rc::new(RefCell::new(Some("?team=blue".to_owned())));
    let followed = route.clone();
    let session = refusing(&signed_in, &attempts).with_start(move |_| followed.borrow().clone());
    let (board, cx) = shown(cx, session);
    let start = |board: &Entity<WebBoard>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| board.update(cx, |board, cx| board.tick_session(cx)));
        board.update(cx, |board, _| board.start.url.clone())
    };
    assert_eq!(
        start(&board, cx),
        "https://app.example.com/desktop/?team=blue"
    );
    route.replace(Some("teams/green".into()));
    assert_eq!(
        start(&board, cx),
        "https://app.example.com/desktop/teams/green"
    );
    route.replace(Some("../other/".into()));
    assert_eq!(start(&board, cx), HOME, "outside the scope");
    route.replace(None);
    assert_eq!(start(&board, cx), HOME);
    cx.run_until_parked();
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "a moved start starts no attempt of its own"
    );
}

#[test]
fn another_account_and_a_moved_start_in_one_poll_open_the_start() {
    let now = Now::current();
    let mut session = live(now, grant(now));
    assert_eq!(session.account(Some(2)), [Effect::Close]);
    assert_eq!(
        session.tick(now, true, Some(PAGE.into())),
        [Effect::Acquire(2)]
    );
    assert_eq!(session.route(Some("teams/green"), page()), []);
    session.acquired(2, Ok(grant(now)), now);
    assert_eq!(
        session.installed(2, Stored::Confirmed, now, None),
        [Effect::Show(2, GREEN.into())]
    );
}
