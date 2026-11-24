//! One web view, described once from a board or a native session, and
//! built with the handlers every view installs.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver},
};

use gpui_kit::Window;

use super::{
    AppFolder, EVENT_CAPACITY, Events, PageEvent, capture_visible, install_bridge, serve_app,
};

/// What a view does with an address it was asked to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    Load,
    /// A native session's signal: report it and load nothing.
    #[cfg(target_os = "macos")]
    Signal,
    Refuse,
}

/// One view to build, decided once from a board or a native session: its
/// builder, first address, navigation rule, the board's app files and
/// whether it has developer tools.
pub(super) struct View<'a> {
    pub(super) builder: wry::WebViewBuilder<'a>,
    pub(super) url: &'a str,
    pub(super) admit: Box<dyn Fn(&str) -> Admission>,
    /// The app files and the board they belong to, when the board shows an
    /// app.
    pub(super) app: Option<(&'a AppFolder, &'a str)>,
    pub(super) devtools: bool,
}

/// The page's end of its view's events.
pub(super) struct Inbox {
    pub(super) events: Receiver<PageEvent>,
    pub(super) overflow: Arc<AtomicBool>,
    pub(super) epoch: Arc<AtomicU64>,
}

pub(super) fn channel() -> (Events, Inbox) {
    let (sender, events) = mpsc::sync_channel(EVENT_CAPACITY);
    let overflow = Arc::new(AtomicBool::new(false));
    let epoch = Arc::new(AtomicU64::new(0));
    let reporter = Events {
        sender,
        overflow: overflow.clone(),
        epoch: epoch.clone(),
    };
    let inbox = Inbox {
        events,
        overflow,
        epoch,
    };
    (reporter, inbox)
}

impl View<'_> {
    /// Builds the view as a child of `window` with the handlers every view
    /// installs: its navigation rule, new windows in the browser, no
    /// downloads, load and title reports, and the board's app files when it
    /// shows an app.
    pub(super) fn build(self, events: Events, window: &Window) -> wry::Result<wry::WebView> {
        let View {
            builder,
            url,
            admit,
            app,
            devtools,
        } = self;
        let builder = match app {
            Some((app, board)) => {
                let builder = serve_app(builder, app, board);
                let builder = capture_visible(builder);
                if app.bridge_enabled {
                    install_bridge(builder, &app.origin, events.clone())
                } else {
                    builder
                }
            }
            None => builder,
        };
        let navigation = events.clone();
        let popup = events.clone();
        let download = events.clone();
        let load = events.clone();
        builder
            .with_devtools(devtools)
            .with_navigation_handler(move |url| navigation.admit(admit(&url), url))
            .with_new_window_req_handler(move |url, _| {
                popup.send(PageEvent::NewWindow(url));
                wry::NewWindowResponse::Deny
            })
            .with_download_started_handler(move |_, _| {
                download.send(PageEvent::Download);
                false
            })
            .with_on_page_load_handler(move |event, url| {
                load.send(match event {
                    wry::PageLoadEvent::Started => PageEvent::Loading(url),
                    wry::PageLoadEvent::Finished => PageEvent::Loaded(url),
                });
            })
            .with_document_title_changed_handler(move |title| events.send(PageEvent::Title(title)))
            .with_url(url)
            .build_as_child(window)
    }
}

impl Events {
    /// Whether the view loads `url`, reporting a signal or a refusal.
    pub(super) fn admit(&self, admission: Admission, url: String) -> bool {
        match admission {
            Admission::Load => {
                self.epoch.fetch_add(1, Ordering::Relaxed);
                true
            }
            #[cfg(target_os = "macos")]
            Admission::Signal => {
                self.send(PageEvent::SessionRequired);
                false
            }
            Admission::Refuse => {
                self.send(PageEvent::Blocked(url));
                false
            }
        }
    }
}
