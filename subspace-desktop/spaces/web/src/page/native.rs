use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
};

use gpui_kit::{AnyView, App, AppContext as _, Context, Entity, Window};
use gpui_webview::WebView;

use super::{AppFolder, PageEvent, Profile, Start};
use crate::app::{APP_SCHEME, serve};
use crate::policy::navigation_allowed;

/// Page events wait here until the board's next poll; more are dropped and
/// the board reads the page's state instead.
const EVENT_CAPACITY: usize = 64;
/// The most events the board handles in one poll.
const EVENTS_PER_POLL: usize = 16;

pub(crate) struct Page {
    view: Entity<WebView>,
    events: Receiver<PageEvent>,
    overflow: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    /// WebView2 reads its profile folder from this context while the view
    /// lives.
    #[cfg(windows)]
    _context: wry::WebContext,
}

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
    /// Create the web view as a child of `window`, loading `start`.
    pub(crate) fn open<T: 'static>(
        start: &Start,
        profile: &Profile,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Result<Self, String> {
        let (sender, events) = mpsc::sync_channel(EVENT_CAPACITY);
        let overflow = Arc::new(AtomicBool::new(false));
        let epoch = Arc::new(AtomicU64::new(0));
        let reporter = Events {
            sender,
            overflow: overflow.clone(),
            epoch: epoch.clone(),
        };
        #[cfg(target_os = "macos")]
        let raw = build_macos(start, profile, reporter, window)?;
        #[cfg(windows)]
        let (raw, context) = build_windows(start, profile, reporter, window)?;
        let view = cx.new(|cx| WebView::new(raw, window, cx));
        Ok(Self {
            view,
            events,
            overflow,
            epoch,
            #[cfg(windows)]
            _context: context,
        })
    }

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

#[cfg(target_os = "macos")]
fn build_macos(
    start: &Start,
    profile: &Profile,
    events: Events,
    window: &Window,
) -> Result<wry::WebView, String> {
    use wry::WebViewBuilderExtDarwin as _;
    let store = crate::policy::data_store_id(&profile.product, &profile.board);
    let builder = wry::WebViewBuilder::new().with_data_store_identifier(store);
    configure(builder, start, &profile.board, events)
        .build_as_child(window)
        .map_err(|error| format!("Cannot open the web view: {error}"))
}

#[cfg(windows)]
fn build_windows(
    start: &Start,
    profile: &Profile,
    events: Events,
    window: &Window,
) -> Result<(wry::WebView, wry::WebContext), String> {
    use wry::WebViewBuilderExtWindows as _;
    let directory = profile
        .state_dir
        .as_ref()
        .map(|dir| dir.join("web-boards").join(&profile.board))
        .ok_or("Cannot find a private folder for the page's data.")?;
    let mut context = wry::WebContext::new(Some(directory));
    // An app origin must be a secure context, as on macOS.
    let builder = wry::WebViewBuilder::new_with_web_context(&mut context).with_https_scheme(true);
    let raw = configure(builder, start, &profile.board, events)
        .build_as_child(window)
        .map_err(|error| {
            format!(
                "Cannot open the web view. Check that Microsoft Edge WebView2 is installed: {error}"
            )
        })?;
    Ok((raw, context))
}

/// The handlers every web board installs: navigation limits, new windows
/// in the browser, no downloads, load and title reports, and the board's
/// app files when it shows an app.
fn configure<'a>(
    builder: wry::WebViewBuilder<'a>,
    start: &Start,
    board: &str,
    events: Events,
) -> wry::WebViewBuilder<'a> {
    let origin = start.app.as_ref().map(|app| app.origin.clone());
    let builder = match &start.app {
        Some(app) => {
            let builder = serve_app(builder, app, board);
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
        .with_devtools(cfg!(debug_assertions))
        .with_navigation_handler(move |url| {
            let allowed = navigation_allowed(&url, origin.as_deref());
            if allowed {
                navigation.epoch.fetch_add(1, Ordering::Relaxed);
            }
            if !allowed {
                navigation.send(PageEvent::Blocked(url));
            }
            allowed
        })
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
        .with_url(&start.url)
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
