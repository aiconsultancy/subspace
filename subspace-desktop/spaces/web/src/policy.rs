//! What a web board lets its page do, and what it shows of the page.

use std::time::{Duration, Instant};

use subspace_framework::core::config::web_boards::web_board_url_allowed;

use crate::app::{APP_SCHEME, same_origin};

/// The longest page title the window title and status repeat.
const MAX_TITLE_CHARS: usize = 80;
/// The shortest time between two pages the board opens in the browser.
const EXTERNAL_OPEN_INTERVAL: Duration = Duration::from_secs(1);
const LOCAL_APP: &str = "Local web app";

/// Whether the web view may load `value`. Frames also use `about:`, `blob:`
/// and `data:` documents; the board's own app, at `app_origin`, may load;
/// every other page must be one a web board may start on.
// Only native web views navigate; keep policy tests on unsupported platforms.
#[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
pub(crate) fn navigation_allowed(value: &str, app_origin: Option<&str>) -> bool {
    app_origin.is_some_and(|origin| same_origin(value, origin))
        || url::Url::parse(value).is_ok_and(|url| {
            matches!(url.scheme(), "about" | "blob" | "data") || web_board_url_allowed(&url)
        })
}

/// The address to open in the default browser for `value`: web pages
/// only, so a page cannot start another app through its URL scheme. App
/// addresses only work inside the board.
pub(crate) fn browser_url(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    let web = matches!(url.scheme(), "https" | "http") && url.host().is_some();
    (web && !is_app_page(&url)).then(|| url.into())
}

/// Whether `url` is a web app board's private address, in either engine's
/// form.
fn is_app_page(url: &url::Url) -> bool {
    let prefix = format!("{APP_SCHEME}.");
    url.scheme() == APP_SCHEME || url.host_str().is_some_and(|host| host.starts_with(&prefix))
}

/// Whether the board may open another page in the browser now. A page
/// cannot flood the desktop with browser windows.
pub(crate) fn may_open_externally(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= EXTERNAL_OPEN_INTERVAL)
}

/// A page title fit for the window title: one line, at most
/// [`MAX_TITLE_CHARS`] characters, `None` when blank.
pub(crate) fn page_title(raw: &str) -> Option<String> {
    let line = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let words = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.is_empty() {
        return None;
    }
    let mut title = words.chars().take(MAX_TITLE_CHARS).collect::<String>();
    if words.chars().count() > MAX_TITLE_CHARS {
        title.push('…');
    }
    Some(title)
}

/// Where the page is, for the status bar: the host, and the port when it
/// is not the scheme's default, or that it is the board's local app. The
/// status shows the real site, whatever the page calls itself.
pub(crate) fn location(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    if is_app_page(&url) {
        return Some(LOCAL_APP.to_owned());
    }
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

/// The spoken name of [`location`]'s text where it stands alone, as in the
/// Space's toolbar: a host is named as the page's site.
pub(crate) fn spoken_location(place: &str) -> String {
    if place == LOCAL_APP {
        place.to_owned()
    } else {
        format!("Page on {place}")
    }
}

/// The status line for a navigation the board refused.
pub(crate) fn blocked_message(value: &str) -> String {
    let scheme = value.split_once(':').map_or(value, |(scheme, _)| scheme);
    match scheme {
        "http" => "Blocked an insecure http:// page. Web Spaces load HTTPS, or HTTP on localhost."
            .to_owned(),
        // A web page this Space does not show: one with credentials in its
        // address, or outside a native session's scope.
        "https" => "Blocked a page this Space does not show.".to_owned(),
        _ => format!("Blocked a {scheme}: link. Web Spaces only load web pages."),
    }
}

/// A stable WebKit data store for one board of one app, so each board
/// keeps its own sign-ins and site data across restarts.
#[cfg(target_os = "macos")]
pub(crate) fn data_store_id(product: &str, board: &str) -> [u8; 16] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("subspace-web-board\0{product}\0{board}"));
    let mut id = [0; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        blocked_message, browser_url, location, may_open_externally, navigation_allowed,
        page_title, spoken_location,
    };

    #[test]
    fn pages_and_frame_documents_load_but_other_schemes_do_not() {
        for allowed in [
            "https://example.com/a",
            "http://localhost:3000/",
            "about:blank",
            "about:srcdoc",
            "blob:https://example.com/0c5e",
            "data:text/html,<p>",
        ] {
            assert!(navigation_allowed(allowed, None), "{allowed}");
        }
        for refused in [
            "http://example.com",
            "file:///etc/hosts",
            "mailto:someone@example.com",
            "slack://open",
            "javascript:alert(1)",
            "https://user:pw@example.com",
            "not a url",
        ] {
            assert!(!navigation_allowed(refused, None), "{refused}");
        }
    }

    #[test]
    fn an_app_board_loads_its_own_origin_only() {
        let origin = Some("subspace://notes");
        assert!(navigation_allowed("subspace://notes/items/1", origin));
        assert!(navigation_allowed("https://example.com/", origin));
        assert!(!navigation_allowed("subspace://other/", origin));
        assert!(!navigation_allowed("subspace://notes/", None));
    }

    #[test]
    fn app_pages_never_open_in_the_browser() {
        assert_eq!(browser_url("https://subspace.notes/"), None);
        assert_eq!(browser_url("subspace://notes/"), None);
        assert_eq!(
            location("subspace://notes/a").as_deref(),
            Some("Local web app")
        );
        assert_eq!(
            location("https://subspace.notes/a").as_deref(),
            Some("Local web app")
        );
    }

    #[test]
    fn only_web_pages_open_in_the_browser() {
        assert_eq!(
            browser_url("http://example.com/x").as_deref(),
            Some("http://example.com/x")
        );
        assert!(browser_url("https://example.com").is_some());
        for refused in [
            "mailto:a@b.c",
            "file:///tmp/x",
            "zoommtg://join",
            "about:blank",
        ] {
            assert_eq!(browser_url(refused), None, "{refused}");
        }
    }

    #[test]
    fn external_opens_are_spaced_out() {
        let start = Instant::now();
        assert!(may_open_externally(None, start));
        assert!(!may_open_externally(Some(start), start));
        assert!(!may_open_externally(
            Some(start),
            start + Duration::from_millis(500)
        ));
        assert!(may_open_externally(
            Some(start),
            start + Duration::from_secs(1)
        ));
    }

    #[test]
    fn titles_are_one_bounded_line() {
        assert_eq!(page_title("  ").as_deref(), None);
        assert_eq!(
            page_title(" Inbox\n - \tMail ").as_deref(),
            Some("Inbox - Mail")
        );
        let long = page_title(&"x".repeat(200)).unwrap();
        assert_eq!(long.chars().count(), 81);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn the_location_is_the_real_host() {
        assert_eq!(
            location("https://wiki.example.com/page?q=1").as_deref(),
            Some("wiki.example.com")
        );
        assert_eq!(
            location("http://localhost:5173/").as_deref(),
            Some("localhost:5173")
        );
        assert_eq!(location("about:blank"), None);
    }

    #[test]
    fn a_spoken_location_says_what_a_host_is() {
        assert_eq!(
            spoken_location("wiki.example.com"),
            "Page on wiki.example.com"
        );
        assert_eq!(spoken_location("Local web app"), "Local web app");
    }

    #[test]
    fn blocked_messages_name_the_kind_of_link() {
        assert!(blocked_message("http://example.com").contains("insecure"));
        assert!(blocked_message("mailto:a@b.c").contains("mailto:"));
        assert_eq!(
            blocked_message("https://sign-in.example.net/"),
            "Blocked a page this Space does not show."
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn each_board_of_each_app_has_its_own_data_store() {
        use super::data_store_id;
        assert_eq!(
            data_store_id("subspace", "wiki"),
            data_store_id("subspace", "wiki")
        );
        assert_ne!(
            data_store_id("subspace", "wiki"),
            data_store_id("subspace", "app")
        );
        assert_ne!(
            data_store_id("subspace", "wiki"),
            data_store_id("other", "wiki")
        );
    }
}
