//! `webBoards`: boards that show one web page or a local web app, defined
//! in the config file rather than compiled into an app.
//! [`super::Config::web_boards`] holds them in id order; only apps that
//! accept configured boards show them.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::PathBuf;

use serde_json::Value;

use crate::Secret;

/// The longest web board id and title the config file accepts.
pub const MAX_WEB_BOARD_ID_CHARS: usize = 40;
pub const MAX_WEB_BOARD_TITLE_CHARS: usize = 60;

/// One `webBoards.<id>` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebBoard {
    /// Lower-case letters, digits and `-`; the board's id everywhere else,
    /// such as `boards` and `boardOrder`.
    pub id: String,
    /// The switcher name. The id when the entry sets none.
    pub title: String,
    pub source: WebBoardSource,
    /// `settings`: the board's own values, each a string, number or
    /// Boolean. The board's manifest describes them.
    pub settings: BTreeMap<String, Value>,
    /// `secrets`: the board's own credentials. Like service tokens, they are
    /// ignored when other users can read the config file.
    pub secrets: BTreeMap<String, Secret>,
    /// User grants keyed by app identity, manifest fingerprint and permission.
    pub permissions: BTreeMap<String, bool>,
}

/// What a web board shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebBoardSource {
    /// `url`: a web page, normalized by the URL parser.
    Url(String),
    /// `path`: an absolute folder of built web app files with an
    /// `index.html`, served to the board from its own private origin.
    Folder(PathBuf),
}

/// Whether `id` can name a web board.
pub fn valid_web_board_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_WEB_BOARD_ID_CHARS
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether a web board may show `url`: HTTPS, or HTTP to this computer for
/// local development. Credentials in the URL are refused, so they cannot
/// leak into logs, titles or the agent context.
pub fn web_board_url_allowed(url: &url::Url) -> bool {
    let plain = url.username().is_empty() && url.password().is_none();
    let secure = match url.scheme() {
        "https" => url.host().is_some(),
        "http" => is_loopback(url),
        _ => false,
    };
    plain && secure
}

fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(url::Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{valid_web_board_id, web_board_url_allowed};

    fn allowed(value: &str) -> bool {
        url::Url::parse(value).is_ok_and(|url| web_board_url_allowed(&url))
    }

    #[test]
    fn https_and_local_http_pages_are_allowed() {
        for value in [
            "https://example.com",
            "https://example.com:8443/path?q=1#top",
            "http://localhost:5173/",
            "http://LOCALHOST:3000",
            "http://127.0.0.1:8080",
            "http://127.1.2.3",
            "http://[::1]:4000/app",
        ] {
            assert!(allowed(value), "{value}");
        }
    }

    #[test]
    fn other_schemes_remote_http_and_credentials_are_refused() {
        for value in [
            "http://example.com",
            "http://localhost.example.com",
            "http://10.0.0.1",
            "https://user:secret@example.com",
            "https://user@example.com",
            "http://user@localhost:3000",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,<p>hi</p>",
            "about:blank",
            "ftp://example.com",
        ] {
            assert!(!allowed(value), "{value}");
        }
    }

    #[test]
    fn ids_are_short_lower_case_names() {
        for id in ["grafana", "my-app", "a1", "x"] {
            assert!(valid_web_board_id(id), "{id}");
        }
        for id in [
            "",
            "Grafana",
            "1app",
            "-app",
            "my_app",
            "my app",
            "é",
            &"a".repeat(41),
        ] {
            assert!(!valid_web_board_id(id), "{id}");
        }
    }
}
