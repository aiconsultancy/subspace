//! A web Space signed in with the app's own account, on macOS. A product's
//! trusted native adapter exchanges the account for a site-issued session
//! cookie, and the Space shows the site's [`SessionScope`] in an isolated,
//! in-memory view that holds that cookie, renews it before it ends and
//! has it revoked when the Space lets it go.
//!
//! Only a product composes this, with [`create_with_session`]; a configured
//! web Space is always [`crate::create`]d. No page script, setting or
//! manifest can turn it on, and the account's tokens stay on the adapter's
//! own HTTP path: the cookie reaches only the view's store, as an
//! `HttpOnly` cookie, never a script, an address or a log. The cookie goes
//! in through WebKit's asynchronous cookie store, which never blocks the UI
//! thread. WebView2 has no such path here yet, so the whole capability is
//! compiled for macOS only, here and in the page module.

use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Global, Window};
use subspace_framework::core::config::web_boards::WebBoard as WebBoardConfig;
use subspace_framework::dashboard::DashboardHandle;

use crate::WebBoard;
use crate::page::{Admission, Page, Start};
use crate::pages::Placeholder;

mod board;
mod grant;
mod machine;
mod revocation;
mod scope;
#[cfg(test)]
mod tests;

pub use grant::{
    MAX_SESSION_LIFETIME, MIN_SESSION_LIFETIME, SessionCookie, SessionFailure, SessionGrant,
};
pub(crate) use scope::view_rule;
pub use scope::{InvalidScope, SessionScope};

use machine::{Here, Session, Waiting};
use revocation::{QUIT_BOUND, REVOCATIONS, Revocations};

type Acquire = Box<dyn FnOnce() -> Result<SessionGrant, SessionFailure> + Send>;
/// The product's start route, read on the UI thread at each poll.
type Route = Box<dyn Fn(&App) -> Option<String>>;

/// The account a session belongs to. The product implements it for the
/// shell's `AccountSession`, which a Space cannot name, with a newtype that
/// forwards both methods.
pub trait SessionAccount: Send + 'static {
    /// Changes when another account signs in.
    fn generation(&self) -> u64;
    /// Whether the account is still the one signed in, and not changing.
    fn is_current(&self) -> bool;
}

/// What a product composes: the scope, how to read the account signed in
/// now, its trusted adapter, which turns the account into a grant off the
/// UI thread, and optionally where the view starts.
pub struct NativeSession {
    scope: SessionScope,
    source: Box<dyn Source>,
    route: Route,
}

impl NativeSession {
    pub fn new<A: SessionAccount>(
        scope: SessionScope,
        account: impl Fn(&App) -> Option<A> + 'static,
        acquire: impl Fn(&A) -> Result<SessionGrant, SessionFailure> + Send + Sync + 'static,
    ) -> Self {
        Self {
            scope,
            source: Box::new(Adapter {
                account,
                acquire: Arc::new(acquire),
            }),
            route: Box::new(|_| None),
        }
    }

    /// Starts the view where the app's own state says, rather than at the
    /// scope's home. `route` reads the app on the UI thread each time the
    /// Space polls, and gives a reference relative to the home, such as
    /// `?team=blue` or `teams/blue`, or `None` for the home itself. When
    /// its answer changes, a view showing a page goes to the new start at
    /// once, and a session showing none starts there; Go to the start page
    /// goes there too. A route that leaves the scope, or names the signal
    /// or a reserved route, is logged and counts as the home.
    // qual:api — a product follows its own state; the Space's tests cover it.
    #[must_use]
    pub fn with_start(mut self, route: impl Fn(&App) -> Option<String> + 'static) -> Self {
        self.route = Box::new(route);
        self
    }
}

/// The product's account and adapter, without their types.
trait Source {
    /// The current account's generation, or `None` while there is none.
    fn generation(&self, cx: &App) -> Option<u64>;
    /// An attempt for the current account.
    fn attempt(&self, cx: &App) -> Option<Acquire>;
}

struct Adapter<F, G> {
    account: F,
    acquire: Arc<G>,
}

impl<A, F, G> Source for Adapter<F, G>
where
    A: SessionAccount,
    F: Fn(&App) -> Option<A>,
    G: Fn(&A) -> Result<SessionGrant, SessionFailure> + Send + Sync + 'static,
{
    fn generation(&self, cx: &App) -> Option<u64> {
        (self.account)(cx)
            .filter(A::is_current)
            .map(|account| account.generation())
    }

    fn attempt(&self, cx: &App) -> Option<Acquire> {
        let account = (self.account)(cx).filter(A::is_current)?;
        let acquire = self.acquire.clone();
        Some(Box::new(move || acquire(&account)))
    }
}

/// A web Space showing `session`'s scope, signed in with its account.
/// `board` names the Space, and its address, when it is a web page, is
/// what Open in browser opens: the site's ordinary page, which signs in on
/// its own.
// qual:api — products compose it; the Space's tests build the same board through `WebBoard::with_session`.
pub fn create_with_session(
    board: &WebBoardConfig,
    session: NativeSession,
    window: &mut Window,
    cx: &mut App,
) -> DashboardHandle {
    revoke_on_quit(cx, &REVOCATIONS);
    let config = board.clone();
    DashboardHandle::from_entity(cx.new(|cx| WebBoard::with_session(config, session, window, cx)))
}

/// Registered once per app: quitting revokes every live session and waits,
/// bounded, for the revocations. GPUI waits only briefly for quit futures,
/// so the wait happens before the future.
struct RevokeOnQuit;

impl Global for RevokeOnQuit {}

fn revoke_on_quit(cx: &mut App, revocations: &'static Revocations) {
    if cx.has_global::<RevokeOnQuit>() {
        return;
    }
    cx.set_global(RevokeOnQuit);
    cx.on_app_quit(move |_| {
        revocations.revoke_all(QUIT_BOUND);
        async {}
    })
    .detach();
}

/// The board's side of a session: the state machine, the product's
/// account, adapter and start route, the window that new views open in,
/// and the view opened for an attempt until the session shows it.
pub(crate) struct SessionPage {
    machine: Session,
    source: Box<dyn Source>,
    route: Route,
    /// The route's answer at the last poll.
    routed: Option<String>,
    window: AnyWindowHandle,
    /// The view opened for an attempt, until the session shows it, closes
    /// or opens a newer one.
    staged: Option<(u64, Page)>,
}

impl SessionPage {
    fn new(session: NativeSession, title: &str, window: AnyWindowHandle) -> Self {
        Self {
            machine: Session::new(session.scope, title),
            source: session.source,
            route: session.route,
            routed: None,
            window,
            staged: None,
        }
    }

    /// Where the view starts: the scope's home or the product's start
    /// under it, not the board's address, with no app.
    pub(crate) fn start(&self) -> Start {
        Start {
            url: self.machine.start().to_owned(),
            app: None,
        }
    }

    /// The product's route, when its answer changed since the last poll.
    fn moved_route(&mut self, cx: &App) -> Option<Option<String>> {
        let route = (self.route)(cx);
        (route != self.routed).then(|| {
            self.routed.clone_from(&route);
            route
        })
    }

    /// Whether Reload means Retry.
    pub(crate) fn needs_retry(&self) -> bool {
        self.machine.needs_retry()
    }

    /// What the Space shows while it has no page. `browser` says whether
    /// Open in browser has somewhere to go.
    pub(crate) fn placeholder(&self, title: &str, browser: bool) -> Placeholder {
        match self.machine.waiting() {
            Some(Waiting::SignIn) => Placeholder::SignIn(sign_in_hint(title, browser)),
            Some(Waiting::Ended(message)) => Placeholder::Failed(message.into()),
            Some(Waiting::Opening) | None => Placeholder::Opening,
        }
    }

    /// The view's address `url`, when it is a page in the scope.
    fn place(&self, url: Option<String>) -> Here {
        url.filter(|url| url != "about:blank" && self.machine.scope().admit(url) == Admission::Load)
    }

    /// The view opened for `attempt`, if it is the one staged.
    fn take_staged(&mut self, attempt: u64) -> Option<Page> {
        self.staged
            .take_if(|(staged, _)| *staged == attempt)
            .map(|(_, page)| page)
    }
}

fn sign_in_hint(title: &str, browser: bool) -> String {
    let or_browser = match browser {
        true => ", or open it in your browser",
        false => "",
    };
    format!("Sign in under Settings › Account to see {title} here{or_browser}.")
}
