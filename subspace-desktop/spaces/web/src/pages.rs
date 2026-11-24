//! Where a web Space's page comes from: the address or folder app the
//! config file names, or, on macOS, a product's native session. The board
//! asks this module where its view starts, whether it has a bridge, to
//! open, retry and reload its page, where Open in browser goes and what to
//! show while there is no page.

use gpui_kit::{App, Context, SharedString, Window};
use subspace_framework::core::config::web_boards::WebBoard as WebBoardConfig;

use crate::WebBoard;
use crate::bridge::Bridge;
use crate::page::Start;
use crate::policy::browser_url;
use crate::toolbar::PageMove;

pub(crate) enum PageSource {
    /// The page or folder app the config file names.
    Config,
    /// A product's native session (`crate::browser_session`).
    #[cfg(target_os = "macos")]
    Session(Box<crate::browser_session::SessionPage>),
}

impl PageSource {
    /// Where the view starts: the config file's page or app, or a
    /// session's scope, whatever the board's address.
    pub(crate) fn start(&self, config: &WebBoardConfig) -> Start {
        match self {
            Self::Config => crate::source::start(config),
            #[cfg(target_os = "macos")]
            Self::Session(session) => session.start(),
        }
    }

    /// The page's bridge, as the config file describes it. A session's
    /// view has none.
    pub(crate) fn bridge(&self, config: &WebBoardConfig, cx: &App) -> Option<Bridge> {
        match self {
            Self::Config => Bridge::new(config, crate::source::entry(&config.id, cx).as_ref()),
            #[cfg(target_os = "macos")]
            Self::Session(_) => None,
        }
    }

    /// The session, for the board to step and apply its effects while it
    /// changes its own page and state.
    // qual:allow(srp, nms) reason: "It returns a mutable borrow of the matched session, which the heuristic does not follow."
    #[cfg(target_os = "macos")]
    pub(crate) fn session(&mut self) -> Option<&mut crate::browser_session::SessionPage> {
        match self {
            Self::Session(session) => Some(session.as_mut()),
            Self::Config => None,
        }
    }
}

/// What the Space shows while it has no page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Placeholder {
    Opening,
    /// Why there is no page, with Retry.
    Failed(SharedString),
    /// A native session waits for the account, with this hint.
    #[cfg(target_os = "macos")]
    SignIn(String),
}

// qual:allow(coupling, oi) reason: "The page's source belongs to the same retained Web Space entity, which owns its page, error and status."
impl WebBoard {
    /// Opens the page, the first time the Space is shown or after Retry.
    pub(crate) fn ensure_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.source {
            PageSource::Config => self.open_configured(window, cx),
            #[cfg(target_os = "macos")]
            PageSource::Session(_) => self.tick_session(cx),
        }
    }

    /// The person chose Retry.
    pub(crate) fn retry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        match &self.source {
            PageSource::Config => self.open_configured(window, cx),
            #[cfg(target_os = "macos")]
            PageSource::Session(_) => self.retry_session(cx),
        }
        self.sync_shown(cx);
    }

    /// Reload means Retry while there is nothing to reload, or a session
    /// has ended.
    pub(crate) fn reload_or_retry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let retry = self.page.is_none()
            || match &self.source {
                PageSource::Config => false,
                #[cfg(target_os = "macos")]
                PageSource::Session(session) => session.needs_retry(),
            };
        match retry {
            true => self.retry(window, cx),
            false => self.move_page(PageMove::Reload, cx),
        }
    }

    /// What Open in browser opens: the page the view shows or, for a
    /// session, whose pages work only in the view, the board's own page.
    pub(crate) fn browser_target(&self, cx: &App) -> Option<String> {
        match &self.source {
            PageSource::Config => self
                .page
                .as_ref()?
                .url(cx)
                .and_then(|url| browser_url(&url)),
            #[cfg(target_os = "macos")]
            PageSource::Session(_) => self.board_page(),
        }
    }

    pub(crate) fn placeholder(&self) -> Placeholder {
        match &self.source {
            PageSource::Config => self
                .error
                .clone()
                .map_or(Placeholder::Opening, Placeholder::Failed),
            #[cfg(target_os = "macos")]
            PageSource::Session(session) => {
                session.placeholder(&self.config.title, self.board_page().is_some())
            }
        }
    }
}
