//! The board with and without a session: a configured web Space never has
//! one; a composed one asks for sign-in, opens its browser target, shows a
//! failure, treats Reload as Retry, drops a grant for a stale account,
//! resumes only at a page in its scope, ends a session whose view is gone
//! when it goes live, and revokes live sessions when the app quits. No
//! test opens a real web view.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use subspace_framework::core::config::web_boards::{WebBoard as WebBoardConfig, WebBoardSource};
use subspace_framework::dashboard::Dashboard as _;

use super::super::grant::Now;
use super::super::machine::{Effect, NOT_INSTALLED, Stored};
use super::super::revocation::Revocations;
use super::super::{NativeSession, SessionAccount, SessionFailure, revoke_on_quit};
use super::{HOME, PAGE, WEBSITE, grant, later, revocable, revoked_within, scope};
use crate::WebBoard;
use crate::page::PageEvent;
use crate::pages::{PageSource, Placeholder};

const REFUSED: &str = "The fixture site refused the account.";

struct Account(u64);

impl SessionAccount for Account {
    fn generation(&self) -> u64 {
        self.0
    }

    fn is_current(&self) -> bool {
        true
    }
}

fn config() -> WebBoardConfig {
    WebBoardConfig {
        id: "desk".to_owned(),
        title: "Desk".to_owned(),
        source: WebBoardSource::Url(WEBSITE.to_owned()),
        settings: Default::default(),
        secrets: Default::default(),
        permissions: Default::default(),
    }
}

/// The account generation signed in, `0` for none.
fn account(signed_in: &Rc<Cell<u64>>) -> impl Fn(&gpui_kit::App) -> Option<Account> + 'static {
    let signed_in = signed_in.clone();
    move |_| {
        Some(signed_in.get())
            .filter(|generation| *generation > 0)
            .map(Account)
    }
}

/// A session whose adapter fails, counting its attempts.
fn refusing(signed_in: &Rc<Cell<u64>>, attempts: &Arc<AtomicUsize>) -> NativeSession {
    let attempts = attempts.clone();
    NativeSession::new(scope(), account(signed_in), move |_: &Account| {
        attempts.fetch_add(1, Ordering::SeqCst);
        Err(SessionFailure::action(REFUSED))
    })
}

/// The board with `session`, just activated in a test window.
fn activated(
    cx: &mut TestAppContext,
    session: NativeSession,
) -> (Entity<WebBoard>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let (board, cx) =
        cx.add_window_view(|window, cx| WebBoard::with_session(config(), session, window, cx));
    cx.update(|window, cx| board.update(cx, |board, cx| board.activate(window, cx)));
    (board, cx)
}

/// The board with `session`, shown, once its first attempt has settled.
fn shown(
    cx: &mut TestAppContext,
    session: NativeSession,
) -> (Entity<WebBoard>, &mut VisualTestContext) {
    let (board, cx) = activated(cx, session);
    cx.run_until_parked();
    (board, cx)
}

#[gpui_kit::test]
fn a_configured_web_space_has_no_session(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (board, cx) = cx.add_window_view(|_, cx| WebBoard::with_config(config(), cx));
    cx.update(|_, cx| {
        board.update(cx, |board, cx| {
            assert!(matches!(board.source, PageSource::Config));
            assert_eq!(board.placeholder(), Placeholder::Opening);
            assert_eq!(
                board.browser_target(cx),
                None,
                "Open in browser needs a page"
            );
            let before = board.state.clone();
            board.page_event(PageEvent::SessionRequired, cx);
            assert_eq!(board.state, before, "no session to renew");
        });
    });
}

#[gpui_kit::test]
fn signed_out_the_space_asks_for_sign_in_and_offers_the_site(cx: &mut TestAppContext) {
    let signed_in = Rc::new(Cell::new(0));
    let attempts = Arc::new(AtomicUsize::new(0));
    let (board, cx) = shown(cx, refusing(&signed_in, &attempts));
    cx.update(|window, cx| {
        window.render_frame(cx);
        let bar = window.within("web-toolbar");
        assert_ne!(bar.find("web-browser").disabled(), Some(true));
        assert_eq!(bar.find("web-home").disabled(), Some(true));
        board.update(cx, |board, cx| {
            assert_eq!(
                board.placeholder(),
                Placeholder::SignIn(
                    "Sign in under Settings › Account to see Desk here, or open it in your browser."
                        .into()
                )
            );
            assert_eq!(board.browser_target(cx).as_deref(), Some(WEBSITE));
            assert!(board.bridge.is_none());
            assert_eq!(board.start.url, HOME);
        });
    });
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        0,
        "nothing while signed out"
    );
}

#[gpui_kit::test]
fn a_failure_shows_its_reason_and_reload_retries(cx: &mut TestAppContext) {
    let signed_in = Rc::new(Cell::new(1));
    let attempts = Arc::new(AtomicUsize::new(0));
    let (board, cx) = shown(cx, refusing(&signed_in, &attempts));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    board.update(cx, |board, _| {
        assert_eq!(board.placeholder(), Placeholder::Failed(REFUSED.into()));
    });
    cx.update(|window, cx| board.update(cx, |board, cx| board.reload(window, cx)));
    cx.run_until_parked();
    assert_eq!(attempts.load(Ordering::SeqCst), 2, "Reload means Retry");

    signed_in.set(2);
    cx.update(|_, cx| board.update(cx, |board, cx| board.tick_session(cx)));
    cx.run_until_parked();
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        3,
        "another account starts again by itself"
    );
}

#[gpui_kit::test]
fn a_grant_for_an_account_that_signed_out_meanwhile_is_revoked_unused(cx: &mut TestAppContext) {
    let (sender, received) = mpsc::channel();
    let signed_in = Rc::new(Cell::new(1));
    let session = NativeSession::new(scope(), account(&signed_in), move |_: &Account| {
        Ok(revocable(Now::current(), &sender, "stale"))
    });
    let (board, cx) = activated(cx, session);
    signed_in.set(0);
    cx.run_until_parked();
    assert_eq!(revoked_within(&received), Some("stale"));
    board.update(cx, |board, _| assert!(board.page.is_none()));
}

#[gpui_kit::test]
fn the_view_resumes_only_at_a_page_in_the_scope(cx: &mut TestAppContext) {
    let signed_in = Rc::new(Cell::new(0));
    let attempts = Arc::new(AtomicUsize::new(0));
    let (board, cx) = shown(cx, refusing(&signed_in, &attempts));
    board.update(cx, |board, _| {
        let session = board.source.session().expect("a session");
        assert_eq!(session.place(Some(PAGE.into())).as_deref(), Some(PAGE));
        for elsewhere in [
            "about:blank",
            "https://app.example.com/desktop/session/required",
            "https://app.example.com/desktop/session/end",
            "https://app.example.com/",
            "https://sign-in.example.net/desktop/",
        ] {
            assert_eq!(session.place(Some(elsewhere.into())), None, "{elsewhere}");
        }
        assert_eq!(session.place(None), None);
    });
}

#[gpui_kit::test]
fn a_view_gone_when_its_session_goes_live_ends_it_and_retry_resumes_there(cx: &mut TestAppContext) {
    let (sender, received) = mpsc::channel();
    let signed_in = Rc::new(Cell::new(1));
    let attempts = Arc::new(AtomicUsize::new(0));
    let (board, cx) = shown(cx, refusing(&signed_in, &attempts));
    let now = Now::current();
    cx.update(|_, cx| {
        board.update(cx, |board, cx| {
            // Live at the home page, then expired while the view showed
            // PAGE; Retry's grant is confirmed, but no view was opened.
            let machine = &mut board.source.session().expect("a session").machine;
            machine.retry();
            machine.acquired(2, Ok(grant(now)), now);
            machine.installed(2, Stored::Confirmed, now, None);
            machine.tick(later(now, 900), true, Some(PAGE.into()));
            machine.retry();
            machine.acquired(3, Ok(revocable(now, &sender, "unshown")), now);
            board.step(
                |machine, now, here| machine.installed(3, Stored::Confirmed, now, here),
                cx,
            );
            assert!(board.page.is_none());
            assert_eq!(
                board.placeholder(),
                Placeholder::Failed(NOT_INSTALLED.into()),
                "not live behind Opening the page…"
            );

            let machine = &mut board.source.session().expect("a session").machine;
            assert_eq!(machine.retry(), [Effect::Acquire(4)]);
            machine.acquired(4, Ok(grant(now)), now);
            assert_eq!(
                machine.installed(4, Stored::Confirmed, now, None),
                [Effect::Show(4, PAGE.into())]
            );
        });
    });
    assert_eq!(revoked_within(&received), Some("unshown"));
}

#[gpui_kit::test]
fn quitting_revokes_every_live_session(cx: &mut TestAppContext) {
    let revocations: &'static Revocations = Box::leak(Box::new(Revocations::new()));
    let (sender, received) = mpsc::channel();
    let _live = revocations.register(Box::new(move || {
        std::thread::sleep(Duration::from_millis(300));
        let _ = sender.send("quit");
    }));
    cx.update(|cx| {
        revoke_on_quit(cx, revocations);
        revoke_on_quit(cx, revocations);
    });
    cx.quit();
    assert_eq!(
        received.try_recv(),
        Ok("quit"),
        "quitting returned only after the slow revocation finished"
    );
    assert!(
        received.recv_timeout(Duration::from_millis(500)).is_err(),
        "once"
    );
}

#[gpui_kit::test]
fn an_idle_session_does_not_redraw_its_space(cx: &mut TestAppContext) {
    let signed_in = Rc::new(Cell::new(0));
    let attempts = Arc::new(AtomicUsize::new(0));
    let (board, cx) = shown(cx, refusing(&signed_in, &attempts));
    let redraws = Rc::new(Cell::new(0));
    let counted = redraws.clone();
    cx.update(|_, cx| {
        cx.observe(&board, move |_, _| counted.set(counted.get() + 1))
            .detach();
    });
    for _ in 0..3 {
        cx.update(|_, cx| board.update(cx, |board, cx| board.tick_session(cx)));
    }
    cx.run_until_parked();
    assert_eq!(redraws.get(), 0, "nothing changed while signed out");

    signed_in.set(1);
    cx.update(|_, cx| board.update(cx, |board, cx| board.tick_session(cx)));
    cx.run_until_parked();
    assert!(redraws.get() > 0, "signing in changes what the Space shows");
}
