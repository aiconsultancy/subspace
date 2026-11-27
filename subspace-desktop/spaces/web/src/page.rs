//! The native web view of one board: WKWebView on macOS, WebView2 on
//! Windows. Other platforms have no page; the board says so instead.

/// What the page reported since the board last looked.
// The unsupported page never emits native web-view events.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PageEvent {
    Loading(String),
    Loaded(String),
    Title(String),
    /// The board refused to load this address.
    Blocked(String),
    /// The page asked for a new window, which opens in the browser.
    NewWindow(String),
    Download,
    #[cfg(all(feature = "agentsview", any(target_os = "macos", windows)))]
    AgentsView(crate::agentsview::ConnectionEvent),
    /// A native session's site sent the view to its signal, which the view
    /// never loads (`SessionScope::with_signal`).
    #[cfg(target_os = "macos")]
    SessionRequired,
    Message {
        source: String,
        body: String,
        epoch: u64,
    },
}

/// What the page loads first.
pub(crate) struct Start {
    pub url: String,
    /// The board's app, when it shows one from a folder.
    pub app: Option<AppFolder>,
}

/// A folder of built web app files and the private origin it is served at.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub(crate) struct AppFolder {
    pub root: std::path::PathBuf,
    pub origin: String,
    pub bridge_enabled: bool,
}

/// Where a board keeps its page's cookies, storage and cache.
pub(crate) struct Profile {
    /// The app's binary name, which separates boards of different apps.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub product: String,
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    pub board: String,
    /// The app's private state directory.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub state_dir: Option<std::path::PathBuf>,
}

impl Profile {
    pub(crate) fn for_board(board: &str, cx: &gpui_kit::App) -> Self {
        let product = cx
            .try_global::<subspace_framework::dashboard::CurrentProduct>()
            .map_or_else(
                || "subspace".to_owned(),
                |product| product.0.product.binary_name.as_str().to_owned(),
            );
        Self {
            product,
            board: board.to_owned(),
            state_dir: subspace_framework::dashboard::prefs::state_dir(cx),
        }
    }
}

/// A native session's cookie as the web view's store holds it: host-only
/// on `domain`, `Secure`, `HttpOnly` and `SameSite=Strict`, until `expires`.
/// The value never leaves native code.
#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteCookie {
    pub name: String,
    pub value: subspace_framework::core::Secret,
    pub domain: String,
    pub path: String,
    pub expires: std::time::SystemTime,
}

#[cfg(any(target_os = "macos", windows))]
mod native;
#[cfg(target_os = "macos")]
pub(crate) use native::Admission;
#[cfg(all(target_os = "macos", debug_assertions))]
mod snapshot;

#[cfg(any(target_os = "macos", windows))]
pub(crate) use native::Page;

#[cfg(not(any(target_os = "macos", windows)))]
mod unsupported;

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) use unsupported::Page;
