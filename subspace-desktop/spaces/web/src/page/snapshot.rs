//! Debug capture builds only: a PNG of what the WKWebView shows. GPUI's
//! window image does not contain native views, so the capture harness would
//! otherwise show an empty Space.

use std::sync::{Arc, Mutex};

use block2::RcBlock;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
use objc2_foundation::{NSDictionary, NSError};
use objc2_web_kit::WKWebView;
use wry::WebViewExtMacOS as _;

/// The latest finished snapshot, and whether one is being taken.
#[derive(Default)]
pub(crate) struct Snapshots {
    latest: Option<Vec<u8>>,
    pending: bool,
}

/// Starts a snapshot unless one is in flight. WebKit calls back on the main
/// thread when the page has drawn.
// qual:allow(complexity, unsafe) reason: "WebKit's snapshot call and its completion block are Objective-C; the block reads a null-checked NSImage on the UI thread."
pub(crate) fn request(webview: &wry::WebView, into: &Arc<Mutex<Snapshots>>) {
    {
        let Ok(mut state) = into.lock() else { return };
        if state.pending {
            return;
        }
        state.pending = true;
    }
    let into = into.clone();
    let done = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
        // SAFETY: WebKit passes a valid NSImage or null for the call's duration.
        let png = unsafe { image.as_ref() }.and_then(png);
        if let Ok(mut state) = into.lock() {
            state.pending = false;
            if png.is_some() {
                state.latest = png;
            }
        }
    });
    let view = webview.webview();
    // Wry's view is a WKWebView; this crate turns on the WebKit features used.
    let view: &WKWebView = &view;
    // SAFETY: called on the UI thread that owns the view; a null configuration
    // snapshots the visible bounds.
    unsafe { view.takeSnapshotWithConfiguration_completionHandler(None, &done) };
}

/// Runs before the page's own scripts in a capture build. The capture app
/// runs behind other windows, where WebKit treats the page as hidden and
/// stops its animation frames, so a capture would show a page that never
/// drew. The page is told it is visible and animation frames come from a
/// timer instead.
pub(crate) const VISIBLE_SCRIPT: &str = "(() => {\
  Object.defineProperty(Document.prototype, 'hidden', { get: () => false });\
  Object.defineProperty(Document.prototype, 'visibilityState', { get: () => 'visible' });\
  window.requestAnimationFrame = callback => setTimeout(() => callback(performance.now()), 16);\
  window.cancelAnimationFrame = id => clearTimeout(id);\
})();";

/// Keeps WebKit from throttling the capture page while its window is behind
/// others, through WebKit's switches for this, when it offers them.
// qual:allow(complexity, unsafe) reason: "WebKit's background-throttling switches are private Objective-C setters, each called only when the object answers it."
pub(crate) fn keep_drawing(webview: &wry::WebView) {
    use objc2::runtime::NSObjectProtocol as _;
    use objc2::sel;
    let view = webview.webview();
    // Wry's view is a WKWebView; this crate turns on the WebKit features used.
    let view: &WKWebView = &view;
    // SAFETY: each private setter is called only when the object answers
    // it; every one takes a single BOOL. Debug capture builds only.
    unsafe {
        if view.respondsToSelector(sel!(_setWindowOcclusionDetectionEnabled:)) {
            let _: () = objc2::msg_send![view, _setWindowOcclusionDetectionEnabled: false];
        }
        let preferences = view.configuration().preferences();
        if preferences.respondsToSelector(sel!(_setHiddenPageDOMTimerThrottlingEnabled:)) {
            let _: () =
                objc2::msg_send![&*preferences, _setHiddenPageDOMTimerThrottlingEnabled: false];
        }
        if preferences.respondsToSelector(sel!(_setPageVisibilityBasedProcessSuppressionEnabled:)) {
            let _: () = objc2::msg_send![&*preferences, _setPageVisibilityBasedProcessSuppressionEnabled: false];
        }
    }
}

/// The finished snapshot, once.
pub(crate) fn take(from: &Arc<Mutex<Snapshots>>) -> Option<Vec<u8>> {
    from.lock().ok()?.latest.take()
}

fn png(image: &NSImage) -> Option<Vec<u8>> {
    let tiff = image.TIFFRepresentation()?;
    let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let properties = NSDictionary::new();
    // SAFETY: an empty property dictionary is valid for PNG.
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }?;
    Some(data.to_vec())
}
