use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{Receiver, SyncSender},
};

use gpui_kit::{AnyView, App, AppContext as _, Context, Entity, Window};
use gpui_webview::WebView;

use super::{AppFolder, PageEvent, Profile, Start};
use crate::app::{APP_SCHEME, serve};
use crate::policy::navigation_allowed;

#[cfg(target_os = "macos")]
mod cookies;
#[cfg(target_os = "macos")]
mod session;
mod view;

pub(crate) use view::Admission;
use view::{Inbox, View, channel};

/// Page events wait here until the board's next poll; more are dropped and
/// the board reads the page's state instead.
const EVENT_CAPACITY: usize = 64;
/// The most events the board handles in one poll.
const EVENTS_PER_POLL: usize = 16;

pub(crate) struct Page {
    view: Entity<WebView>,
    #[cfg(all(target_os = "macos", debug_assertions))]
    snapshots: Arc<std::sync::Mutex<super::snapshot::Snapshots>>,
    events: Receiver<PageEvent>,
    overflow: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    _store: Store,
}

/// What the view needs kept while it lives: on Windows the context
/// WebView2 reads its profile folder from.
#[cfg(windows)]
type Store = wry::WebContext;
#[cfg(target_os = "macos")]
type Store = ();

#[derive(Clone)]
struct Events {
    sender: SyncSender<PageEvent>,
    overflow: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}

impl Events {
    fn send(&self, event: PageEvent) {
        if self.sender.try_send(event).is_err() {
            self.overflow.store(true, Ordering::Relaxed);
        }
    }
}

impl Page {
    /// Create a board's web view as a child of `window`, loading `start`,
    /// with the board's own site data store.
    pub(crate) fn open<T: 'static>(
        start: &Start,
        profile: &Profile,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Result<Self, String> {
        let origin = start.app.as_ref().map(|app| app.origin.clone());
        let view = |builder| View {
            builder,
            url: &start.url,
            admit: Box::new(
                move |url: &str| match navigation_allowed(url, origin.as_deref()) {
                    true => Admission::Load,
                    false => Admission::Refuse,
                },
            ),
            app: start.app.as_ref().map(|app| (app, profile.board.as_str())),
            devtools: cfg!(debug_assertions),
        };
        let (events, inbox) = channel();
        #[cfg(target_os = "macos")]
        let (raw, store) = {
            use wry::WebViewBuilderExtDarwin as _;
            let id = crate::policy::data_store_id(&profile.product, &profile.board);
            let builder = wry::WebViewBuilder::new().with_data_store_identifier(id);
            let raw = view(builder).build(events, window);
            (
                raw.map_err(|error| format!("Cannot open the web view: {error}"))?,
                (),
            )
        };
        #[cfg(windows)]
        let (raw, store) = {
            use wry::WebViewBuilderExtWindows as _;
            let mut context = web_context(profile)?;
            // An app origin must be a secure context, as on macOS.
            let builder =
                wry::WebViewBuilder::new_with_web_context(&mut context).with_https_scheme(true);
            let raw = view(builder).build(events, window).map_err(|error| {
                format!(
                    "Cannot open the web view. Check that Microsoft Edge WebView2 is installed: {error}"
                )
            })?;
            (raw, context)
        };
        Ok(Self::adopt(raw, inbox, store, window, cx))
    }

    /// The page around a built view.
    fn adopt<T: 'static>(
        raw: wry::WebView,
        inbox: Inbox,
        store: Store,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        #[cfg(all(target_os = "macos", debug_assertions))]
        if std::env::var_os("SUBSPACE_CAPTURE_DIR").is_some() {
            super::snapshot::keep_drawing(&raw);
        }
        Self {
            view: cx.new(|cx| WebView::new(raw, window, cx)),
            #[cfg(all(target_os = "macos", debug_assertions))]
            snapshots: Arc::default(),
            events: inbox.events,
            overflow: inbox.overflow,
            epoch: inbox.epoch,
            _store: store,
        }
    }

    /// The page's element. The native view takes the bounds GPUI lays this
    /// element out at, so GPUI content that must stay visible, such as the
    /// Space's toolbar, sits outside it.
    pub(crate) fn view(&self) -> AnyView {
        self.view.clone().into()
    }

    /// Show or hide the native view, which draws above everything GPUI
    /// draws in its bounds.
    pub(crate) fn set_shown(&self, shown: bool, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            if shown && !view.visible() {
                view.show();
                cx.notify();
            } else if !shown && view.visible() {
                view.hide();
            }
        });
    }

    /// The events since the last call, and whether some were dropped.
    pub(crate) fn take_events(&self) -> (Vec<PageEvent>, bool) {
        let events = self.events.try_iter().take(EVENTS_PER_POLL).collect();
        (events, self.overflow.swap(false, Ordering::Relaxed))
    }

    pub(crate) fn url(&self, cx: &App) -> Option<String> {
        self.view.read(cx).raw().url().ok()
    }

    pub(crate) fn is_current(&self, epoch: u64) -> bool {
        self.epoch.load(Ordering::Relaxed) == epoch
    }

    pub(crate) fn respond(&self, epoch: u64, script: &str, cx: &App) {
        if self.is_current(epoch) {
            let _ = self.script(script, cx);
        }
    }

    /// Runs an event script in whatever document the view shows now. The
    /// script checks the document's origin itself before dispatching.
    pub(crate) fn push(&self, script: &str, cx: &App) {
        let _ = self.script(script, cx);
    }

    /// Debug capture only: starts a snapshot of the page and returns the
    /// last finished one as PNG bytes.
    #[cfg(all(target_os = "macos", debug_assertions))]
    pub(crate) fn snapshot(&self, cx: &App) -> Option<Vec<u8>> {
        super::snapshot::request(self.view.read(cx).raw(), &self.snapshots);
        super::snapshot::take(&self.snapshots)
    }

    pub(crate) fn back(&self, cx: &App) -> Result<(), String> {
        self.script("history.back();", cx)
    }

    pub(crate) fn forward(&self, cx: &App) -> Result<(), String> {
        self.script("history.forward();", cx)
    }

    pub(crate) fn reload(&self, cx: &App) -> Result<(), String> {
        self.view
            .read(cx)
            .raw()
            .reload()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn load(&self, url: &str, cx: &App) -> Result<(), String> {
        self.view
            .read(cx)
            .raw()
            .load_url(url)
            .map_err(|error| error.to_string())
    }

    fn script(&self, script: &str, cx: &App) -> Result<(), String> {
        self.view
            .read(cx)
            .raw()
            .evaluate_script(script)
            .map_err(|error| error.to_string())
    }
}

/// The folder WebView2 keeps this board's site data in.
#[cfg(windows)]
fn web_context(profile: &Profile) -> Result<wry::WebContext, String> {
    let directory = profile
        .state_dir
        .as_ref()
        .map(|dir| dir.join("web-boards").join(&profile.board))
        .ok_or("Cannot find a private folder for the page's data.")?;
    Ok(wry::WebContext::new(Some(directory)))
}

/// Debug capture builds only: see `snapshot::VISIBLE_SCRIPT`.
fn capture_visible(builder: wry::WebViewBuilder<'_>) -> wry::WebViewBuilder<'_> {
    #[cfg(all(target_os = "macos", debug_assertions))]
    if std::env::var_os("SUBSPACE_CAPTURE_DIR").is_some() {
        return builder.with_initialization_script(super::snapshot::VISIBLE_SCRIPT);
    }
    builder
}

/// Inject only in the app's main document. Wry supplies the source URI
/// from the native engine; no origin sent by JavaScript is trusted.
fn install_bridge<'a>(
    builder: wry::WebViewBuilder<'a>,
    origin: &str,
    events: Events,
) -> wry::WebViewBuilder<'a> {
    let script = include_str!("../bridge/init.js").replace(
        "\"__SUBSPACE_APP_ORIGIN__\"",
        &serde_json::json!(origin).to_string(),
    );
    let origin = origin.to_owned();
    builder
        .with_initialization_script_for_main_only(script, true)
        .with_ipc_handler(move |request| {
            let source = request.uri().to_string();
            if request.body().len() <= crate::bridge::protocol::MAX_MESSAGE_BYTES
                && crate::app::same_origin(&source, &origin)
            {
                events.send(PageEvent::Message {
                    source,
                    body: request.into_body(),
                    epoch: events.epoch.load(Ordering::Relaxed),
                });
            }
        })
}

/// Serve the board's app folder at its private origin. Each request reads
/// its file on its own thread, off the UI thread.
fn serve_app<'a>(
    builder: wry::WebViewBuilder<'a>,
    app: &AppFolder,
    board: &str,
) -> wry::WebViewBuilder<'a> {
    let root = app.root.clone();
    let bridge_enabled = app.bridge_enabled;
    let board = board.to_owned();
    builder.with_asynchronous_custom_protocol(APP_SCHEME.into(), move |_, request, responder| {
        let (root, board) = (root.clone(), board.clone());
        let method = request.method().as_str().to_owned();
        let uri = request.uri().to_string();
        std::thread::spawn(move || {
            responder.respond(response(
                serve(&root, &board, &method, &uri),
                bridge_enabled,
            ))
        });
    })
}

fn response(served: crate::app::Served, bridge_enabled: bool) -> wry::http::Response<Vec<u8>> {
    let builder = wry::http::Response::builder()
        .status(served.status)
        .header(wry::http::header::CONTENT_TYPE, served.mime)
        .header("X-Content-Type-Options", "nosniff")
        .header(wry::http::header::CACHE_CONTROL, "no-cache");
    let builder = if bridge_enabled {
        builder.header("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data:; connect-src 'self' https: http://localhost:* http://127.0.0.1:* http://[::1]:*; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'")
    } else {
        builder
    };
    builder.body(served.body).unwrap_or_default()
}
