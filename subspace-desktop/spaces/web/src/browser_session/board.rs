//! The board's side of the session: it feeds the state machine the
//! account, the clock and where the view is, and applies the effects each
//! transition returns, in order.

use std::future::Future;
use std::time::Duration;

use gpui_kit::{AppContext as _, Context, FutureExt as _, Timeout, Window};
use subspace_framework::core::config::web_boards::{WebBoard as WebBoardConfig, WebBoardSource};
use subspace_framework::core::log::{Level, Logger};

use super::grant::Now;
use super::machine::{Effect, Here, Session, Stored};
use super::{NativeSession, SessionFailure, SessionPage};
use crate::page::{Page, SiteCookie};
use crate::pages::PageSource;
use crate::{INSTANCE, WebBoard};

/// The longest the Space waits for the adapter's attempt.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(60);
/// The longest the Space waits for the view's cookie store.
const STORE_TIMEOUT: Duration = Duration::from_secs(10);
const ACQUIRE_TIMED_OUT: &str = "Signing in took too long. Try again.";

// qual:allow(coupling, oi) reason: "These private lifecycle methods update the same retained Web Space entity and do not access another Space."
impl WebBoard {
    pub(super) fn with_session(
        config: WebBoardConfig,
        session: NativeSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let page = SessionPage::new(session, &config.title, window.window_handle());
        Self::with_source(config, PageSource::Session(Box::new(page)), cx)
    }

    /// Follows the account and the clock, each poll.
    pub(crate) fn tick_session(&mut self, cx: &mut Context<Self>) {
        let shown = self.active;
        let Some(session) = self.source.session() else {
            return;
        };
        let generation = session.source.generation(cx);
        self.step(
            |machine, now, here| {
                let mut effects = machine.account(generation);
                effects.extend(machine.tick(now, shown, here));
                effects
            },
            cx,
        );
    }

    /// The view was sent to the scope's signal.
    pub(crate) fn signal_session(&mut self, cx: &mut Context<Self>) {
        self.step(|machine, now, here| machine.signal(now, here), cx);
    }

    pub(crate) fn retry_session(&mut self, cx: &mut Context<Self>) {
        self.step(|machine, _, _| machine.retry(), cx);
    }

    /// The board's own address, when it is a web page: what Open in
    /// browser opens for a session, whose pages work only in the view.
    pub(crate) fn board_page(&self) -> Option<String> {
        match &self.config.source {
            WebBoardSource::Url(url) => crate::policy::browser_url(url),
            WebBoardSource::Folder(_) => None,
        }
    }

    /// Runs one transition, with the view where it is now, and applies its
    /// effects, in order. The board redraws only when the transition
    /// changed something.
    pub(super) fn step(
        &mut self,
        transition: impl FnOnce(&mut Session, Now, Here) -> Vec<Effect>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.source.session() else {
            return;
        };
        let here = self
            .page
            .as_ref()
            .and_then(|page| session.place(page.url(cx)));
        let step = session.machine.step(Now::current(), here, transition);
        for effect in step.effects {
            self.apply(effect, cx);
        }
        if step.changed {
            cx.notify();
        }
    }

    fn apply(&mut self, effect: Effect, cx: &mut Context<Self>) {
        match effect {
            Effect::Acquire(attempt) => self.acquire(attempt, cx),
            Effect::Open(attempt, cookie) => self.open_view(attempt, cookie, cx),
            Effect::Install(attempt, cookie) => self.install(attempt, &cookie, cx),
            Effect::Show(attempt, url) => self.show(attempt, url, cx),
            Effect::Discard(attempt) => self.discard(attempt),
            Effect::Renewed => self.report_renewal(cx),
            Effect::Return(url) => self.return_to(&url, cx),
            Effect::Close => self.close_view(cx),
            Effect::Notice(message) => self.state.notice = Some(message),
            Effect::Log(problem) => log(&self.config.id, problem),
        }
    }

    /// Runs the adapter off the UI thread, for at most [`ACQUIRE_TIMEOUT`];
    /// a later answer is dropped, which revokes its grant.
    fn acquire(&mut self, attempt: u64, cx: &mut Context<Self>) {
        let Some(job) = self
            .source
            .session()
            .and_then(|session| session.source.attempt(cx))
        else {
            return;
        };
        cx.spawn(async move |board, cx| {
            let job = cx.background_spawn(async move { job() });
            let result = job
                .with_timeout(ACQUIRE_TIMEOUT, cx.background_executor())
                .await
                .unwrap_or_else(|Timeout| Err(SessionFailure::temporary(ACQUIRE_TIMED_OUT)));
            let _ = board.update(cx, |board, cx| {
                board.tick_session(cx);
                board.step(|machine, now, _| machine.acquired(attempt, result, now), cx);
            });
        })
        .detach();
    }

    /// Opens a blank, isolated view in the board's window, in place of any
    /// older one, and installs the cookie in it; the view is shown only
    /// once the store confirms it.
    fn open_view(&mut self, attempt: u64, cookie: SiteCookie, cx: &mut Context<Self>) {
        let Some(session) = self.source.session() else {
            return;
        };
        session.staged = None;
        let (scope, window) = (session.machine.scope().clone(), session.window);
        cx.spawn(async move |board, cx| {
            let opened = window.update(cx, |_, window, cx| {
                board.update(cx, |board, cx| {
                    match Page::open_session(&scope, window, cx) {
                        Ok(page) => board.stage(attempt, page, &cookie, cx),
                        Err(error) => board.view_failed(attempt, &error, cx),
                    }
                })
            });
            if opened.is_err() {
                let _ = board.update(cx, |board, cx| {
                    board.view_failed(attempt, "The window closed.", cx);
                });
            }
        })
        .detach();
    }

    /// Keeps the view opened for `attempt` and installs the cookie in it,
    /// unless the attempt no longer applies, which drops the view.
    fn stage(&mut self, attempt: u64, page: Page, cookie: &SiteCookie, cx: &mut Context<Self>) {
        let Some(session) = self
            .source
            .session()
            .filter(|session| session.machine.opening(attempt))
        else {
            return;
        };
        let stored = page.install_cookie(cookie, cx);
        session.staged = Some((attempt, page));
        Self::await_store(attempt, stored, cx);
    }

    fn view_failed(&mut self, attempt: u64, error: &str, cx: &mut Context<Self>) {
        log(&self.config.id, "The session's view could not open.");
        self.state.notice = Some(error.to_owned());
        Self::await_store(attempt, async { false }, cx);
    }

    fn install(&mut self, attempt: u64, cookie: &SiteCookie, cx: &mut Context<Self>) {
        match self
            .page
            .as_ref()
            .map(|page| page.install_cookie(cookie, cx))
        {
            Some(stored) => Self::await_store(attempt, stored, cx),
            None => Self::await_store(attempt, async { false }, cx),
        }
    }

    /// Waits, for at most [`STORE_TIMEOUT`], for the store's answer, then
    /// settles the attempt.
    fn await_store(
        attempt: u64,
        stored: impl Future<Output = bool> + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |board, cx| {
            let stored = match stored
                .with_timeout(STORE_TIMEOUT, cx.background_executor())
                .await
            {
                Ok(true) => Stored::Confirmed,
                Ok(false) => Stored::Refused,
                Err(Timeout) => Stored::TimedOut,
            };
            let _ = board.update(cx, |board, cx| {
                board.step(
                    |machine, now, here| machine.installed(attempt, stored, now, here),
                    cx,
                );
            });
        })
        .detach();
    }

    /// Shows the view opened for `attempt`, at `url`. Without that view the
    /// session ends, rather than stay live behind "Opening the page…".
    fn show(&mut self, attempt: u64, url: String, cx: &mut Context<Self>) {
        let Some(page) = self
            .source
            .session()
            .and_then(|session| session.take_staged(attempt))
        else {
            return self.step(|machine, _, _| machine.not_shown(url), cx);
        };
        if let Err(error) = page.load(&url, cx) {
            self.state.notice = Some(error);
        }
        self.page = Some(page);
        self.error = None;
        self.sync_shown(cx);
    }

    /// Drops the view opened for `attempt`, which never showed, with its
    /// store.
    fn discard(&mut self, attempt: u64) {
        if let Some(session) = self.source.session() {
            drop(session.take_staged(attempt));
        }
    }

    fn report_renewal(&self, cx: &mut Context<Self>) {
        if let Some(page) = &self.page {
            page.report_session_renewal(cx);
        }
    }

    /// Loads `url`, the page the view showed, again.
    fn return_to(&mut self, url: &str, cx: &mut Context<Self>) {
        let Some(page) = &self.page else {
            return;
        };
        if let Err(error) = page.load(url, cx) {
            self.state.notice = Some(error);
        }
    }

    /// Closes the view, and any view opened for an attempt.
    fn close_view(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = self.page.take() {
            page.set_shown(false, cx);
        }
        self.state = Default::default();
        if let Some(session) = self.source.session() {
            session.staged = None;
        }
    }
}

/// Records a problem with the product's composition, never a credential.
fn log(board: &str, problem: &str) {
    Logger::new(INSTANCE).record(Level::Error, format!("{board}: {problem}"), &[]);
}
