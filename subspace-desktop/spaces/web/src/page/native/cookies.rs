//! A native session's cookie in the view's WKHTTPCookieStore, without
//! blocking: the call starts on the UI thread and returns at once, WebKit
//! calls back on the UI thread, and a one-shot channel carries the answer
//! to the future the Space awaits. Nothing spins the run loop or waits on
//! the UI thread, so the app is never re-entered from inside an update.

use std::cell::RefCell;
use std::future::Future;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::UNIX_EPOCH;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArray, NSDate, NSHTTPCookie, NSHTTPCookieDomain, NSHTTPCookieExpires, NSHTTPCookieName,
    NSHTTPCookiePath, NSHTTPCookiePropertyKey, NSHTTPCookieSameSitePolicy,
    NSHTTPCookieSameSiteStrict, NSHTTPCookieSecure, NSHTTPCookieValue, NSHTTPCookieVersion,
    NSMutableDictionary, NSString, ns_string,
};
use objc2_web_kit::{WKHTTPCookieStore, WKWebView};
use wry::WebViewExtMacOS as _;

use crate::page::SiteCookie;

/// How far the stored expiry may be from the one the Space set: WebKit
/// keeps it in whole seconds.
const EXPIRY_TOLERANCE_SECONDS: f64 = 1.0;

type Answer = Rc<RefCell<Option<tokio::sync::oneshot::Sender<bool>>>>;

/// Stores `cookie`, then reads the store back once WebKit has stored it.
/// Resolves to whether the store holds the cookie as installed.
// qual:allow(complexity, unsafe) reason: "WebKit's cookie store is Objective-C: one asynchronous call on the UI thread, whose completion block runs on the UI thread."
pub(super) fn install(
    view: &wry::WebView,
    cookie: &SiteCookie,
) -> impl Future<Output = bool> + 'static {
    let (sender, reply) = tokio::sync::oneshot::channel();
    let answer: Answer = Rc::new(RefCell::new(Some(sender)));
    match native_cookie(cookie) {
        Some(native) => {
            let store = cookie_store(view);
            let (reader, expected) = (store.clone(), cookie.clone());
            let stored = RcBlock::new(move || read_back(&reader, expected.clone(), answer.clone()));
            // SAFETY: called on the UI thread that owns the view; WebKit
            // copies the cookie and calls the block once, on the UI thread.
            unsafe { store.setCookie_completionHandler(&native, Some(&stored)) };
        }
        None => send(&answer, false),
    }
    async move { reply.await.unwrap_or(false) }
}

fn cookie_store(view: &wry::WebView) -> Retained<WKHTTPCookieStore> {
    let view = view.webview();
    // Wry's view is a WKWebView; this crate turns on the WebKit features used.
    let view: &WKWebView = &view;
    // SAFETY: called on the UI thread that owns the view.
    unsafe { view.configuration().websiteDataStore().httpCookieStore() }
}

fn send(answer: &Answer, held: bool) {
    if let Some(sender) = answer.borrow_mut().take() {
        let _ = sender.send(held);
    }
}

fn read_back(store: &WKHTTPCookieStore, expected: SiteCookie, answer: Answer) {
    let read = RcBlock::new(move |cookies: NonNull<NSArray<NSHTTPCookie>>| {
        // SAFETY: WebKit passes a valid array for the call's duration.
        let cookies = unsafe { cookies.as_ref() };
        send(
            &answer,
            cookies.iter().any(|stored| matches(&stored, &expected)),
        );
    });
    // SAFETY: called on the UI thread; WebKit calls the block once, on the
    // UI thread.
    unsafe { store.getAllCookies(&read) };
}

/// Whether `stored` is the cookie the Space set, with its protections and
/// its expiry: never a cookie that lasts for the session.
// qual:allow(complexity, unsafe) reason: "Reads Foundation's SameSite=Strict constant, an immutable extern static."
fn matches(stored: &NSHTTPCookie, expected: &SiteCookie) -> bool {
    // SAFETY: Foundation's own constant.
    let strict = unsafe { NSHTTPCookieSameSiteStrict };
    let expires = expected
        .expires
        .duration_since(UNIX_EPOCH)
        .map_or(f64::NAN, |since| since.as_secs_f64());
    stored.name().to_string() == expected.name
        && stored.value().to_string() == expected.value.expose()
        && stored.domain().to_string() == expected.domain
        && stored.path().to_string() == expected.path
        && stored.isSecure()
        && stored.isHTTPOnly()
        && !stored.isSessionOnly()
        && stored.expiresDate().is_some_and(|date| {
            (date.timeIntervalSince1970() - expires).abs() <= EXPIRY_TOLERANCE_SECONDS
        })
        && stored
            .sameSitePolicy()
            .is_some_and(|policy| &*policy == strict)
}

/// The cookie as WebKit stores it: host-only (a domain without a leading
/// dot), `Secure`, `HttpOnly` and `SameSite=Strict`, until it expires.
// qual:allow(complexity, unsafe) reason: "NSHTTPCookie is built from Foundation's property keys, which are extern statics, through its documented properties constructor."
fn native_cookie(cookie: &SiteCookie) -> Option<Retained<NSHTTPCookie>> {
    let expires = cookie
        .expires
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    let name = NSString::from_str(&cookie.name);
    let value = NSString::from_str(cookie.value.expose());
    let domain = NSString::from_str(&cookie.domain);
    let path = NSString::from_str(&cookie.path);
    let expires = NSDate::dateWithTimeIntervalSince1970(expires);
    let yes = ns_string!("TRUE");
    // SAFETY: Foundation's own property keys and values, with an NSString
    // or NSDate for each, as `cookieWithProperties:` documents. WebKit
    // reads the `HttpOnly` key, which has no Foundation constant.
    unsafe {
        let properties: Retained<NSMutableDictionary<NSHTTPCookiePropertyKey, AnyObject>> =
            NSMutableDictionary::from_slices(
                &[
                    NSHTTPCookieName,
                    NSHTTPCookieValue,
                    NSHTTPCookieDomain,
                    NSHTTPCookiePath,
                    NSHTTPCookieSecure,
                    ns_string!("HttpOnly"),
                    NSHTTPCookieSameSitePolicy,
                    NSHTTPCookieVersion,
                ],
                &[
                    &*name,
                    &*value,
                    &*domain,
                    &*path,
                    yes,
                    yes,
                    NSHTTPCookieSameSiteStrict,
                    ns_string!("0"),
                ],
            );
        properties.insert(NSHTTPCookieExpires, &*expires);
        NSHTTPCookie::cookieWithProperties(&properties)
    }
}
