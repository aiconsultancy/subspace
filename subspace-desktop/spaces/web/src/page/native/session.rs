//! A native session's view (`crate::browser_session`): isolated and in
//! memory, loading only its scope, with no app, bridge or developer tools,
//! and its cookie installed through the view's own store without blocking
//! the UI thread.

use std::future::Future;

use gpui_kit::{App, Context, Window};

use super::{Page, View, channel, cookies};
use crate::browser_session::{SessionScope, view_rule};
use crate::page::SiteCookie;

impl Page {
    /// A new view for `scope` with a new in-memory store, blank until the
    /// session's cookie is installed.
    pub(crate) fn open_session<T: 'static>(
        scope: &SessionScope,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Result<Self, String> {
        let view = View {
            builder: wry::WebViewBuilder::new().with_incognito(true),
            url: "about:blank",
            admit: Box::new(view_rule(scope.clone())),
            app: None,
            devtools: false,
        };
        let (events, inbox) = channel();
        let raw = view
            .build(events, window)
            .map_err(|error| format!("Cannot open the web view: {error}"))?;
        Ok(Self::adopt(raw, inbox, (), window, cx))
    }

    /// Stores `cookie` in the view's store, then reads the store back.
    /// Resolves to whether the store holds it exactly as installed; it
    /// never resolves if WebKit never answers, so race it with a timer.
    pub(crate) fn install_cookie(
        &self,
        cookie: &SiteCookie,
        cx: &App,
    ) -> impl Future<Output = bool> + 'static {
        cookies::install(self.view.read(cx).raw(), cookie)
    }

    /// Tells the page, without a payload, that its session was renewed.
    pub(crate) fn report_session_renewal(&self, cx: &App) {
        let _ = self.script(
            "window.dispatchEvent(new Event('subspace-session-renewed'));",
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    };

    use super::super::{Admission, Events};
    use crate::browser_session::SessionScope;
    use crate::page::PageEvent;

    const SIGNAL: &str = "https://app.example.com/desktop/session/required";

    fn events() -> (Events, mpsc::Receiver<PageEvent>) {
        let (sender, received) = mpsc::sync_channel(8);
        let events = Events {
            sender,
            overflow: Arc::new(AtomicBool::new(false)),
            epoch: Arc::new(AtomicU64::new(0)),
        };
        (events, received)
    }

    fn scope() -> SessionScope {
        SessionScope::new("https://app.example.com/desktop/")
            .and_then(|scope| scope.with_signal("session/required"))
            .unwrap()
    }

    #[test]
    fn the_signal_is_reported_and_never_loaded() {
        let (events, received) = events();
        let scope = scope();
        assert!(!events.admit(scope.admit(SIGNAL), SIGNAL.into()));
        assert_eq!(received.try_recv(), Ok(PageEvent::SessionRequired));
        assert_eq!(events.epoch.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_session_view_reports_pages_outside_its_scope_and_counts_loads() {
        let (events, received) = events();
        let scope = scope();
        for refused in [
            "https://app.example.com/",
            "https://app.example.com/desktop/%2e%2e%2fapi",
            "https://sign-in.example.net/desktop/",
            "about:srcdoc",
        ] {
            assert!(!events.admit(scope.admit(refused), refused.into()));
            assert_eq!(received.try_recv(), Ok(PageEvent::Blocked(refused.into())));
        }
        assert_eq!(events.epoch.load(Ordering::Relaxed), 0);
        let page = "https://app.example.com/desktop/reports";
        assert!(events.admit(scope.admit(page), page.into()));
        assert_eq!(events.epoch.load(Ordering::Relaxed), 1);
        assert_eq!(scope.admit(page), Admission::Load);
    }
}
