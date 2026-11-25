//! Where a native session applies: one HTTPS origin, under one path prefix.
//! The view loads nothing else, redirects and frames included, and the
//! session cookie is scoped to the same prefix.

use std::cell::Cell;
use std::fmt;

use url::Url;

use crate::page::Admission;

/// Bytes a percent-escape may not hide from the prefix check: unreserved
/// characters, which servers decode to the same route, and separators.
const HIDDEN_BYTES: &[u8] = b"-._~/\\%;";

/// The pages a web Space with a native session may show: HTTPS pages on
/// `home`'s origin whose path starts with `home`'s path, less any reserved
/// routes. Build it with [`SessionScope::new`], such as
/// `SessionScope::new("https://app.example.com/desktop/")?.with_signal("session/required")?`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionScope {
    home: Url,
    /// The signal's absolute path, in lower case.
    signal: Option<String>,
    /// Absolute paths, in lower case, the view never loads, nor the same
    /// path with a format suffix, nor anything under them.
    reserved: Vec<String>,
}

/// Why a [`SessionScope`] was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidScope(&'static str);

// qual:allow(dry, boilerplate) reason: "The error's Display writes its fixed message, which the scope's tests compare."
impl fmt::Display for InvalidScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for InvalidScope {}

impl SessionScope {
    /// The scope starting at `home`: an `https://` address with a host, no
    /// credentials, query or fragment, whose path ends with `/`. The path is
    /// the prefix every page must sit under, so `https://app.example.com/`
    /// scopes the whole site.
    pub fn new(home: &str) -> Result<Self, InvalidScope> {
        const INVALID: InvalidScope = InvalidScope(
            "A session's home must be an https:// address whose path ends with /, without credentials, query or fragment.",
        );
        let home = Url::parse(home).map_err(|_| INVALID)?;
        let valid = home.scheme() == "https"
            && home.host_str().is_some_and(|host| !host.is_empty())
            && home.username().is_empty()
            && home.password().is_none()
            && home.query().is_none()
            && home.fragment().is_none()
            && home.path().ends_with('/')
            && path_is_plain(home.path());
        valid
            .then_some(Self {
                home,
                signal: None,
                reserved: Vec::new(),
            })
            .ok_or(INVALID)
    }

    /// The route under the home, such as `session/required`, that the site
    /// sends the view to when it no longer accepts the session. The view
    /// never loads it: the Space renews the session at once and returns to
    /// the page, or ends the session if the site sends it there again
    /// before a page loads. Matched ignoring ASCII case and the query.
    // qual:api — a product declares its site's signal; the tests cover it.
    pub fn with_signal(mut self, route: &str) -> Result<Self, InvalidScope> {
        self.signal = Some(self.absolute(route)?);
        Ok(self)
    }

    /// A route under the home, such as the site's own session endpoints,
    /// that the view never navigates to, ignoring ASCII case. Nor does it
    /// go to anything under it, or to the route with a format suffix, such
    /// as `session.json`, which many web frameworks send to the same
    /// handler. It limits navigations only: a page in the scope can still
    /// `fetch` it, with the session cookie.
    // qual:api — a product declares its site's own routes; the tests cover them.
    pub fn with_reserved(mut self, route: &str) -> Result<Self, InvalidScope> {
        let path = self.absolute(route)?;
        self.reserved.push(path.trim_end_matches('/').to_owned());
        Ok(self)
    }

    /// The address the view starts at, and Go to the start page returns to.
    pub fn home(&self) -> &str {
        self.home.as_str()
    }

    pub(crate) fn host(&self) -> &str {
        self.home.host_str().unwrap_or_default()
    }

    /// The path prefix, ending with `/`.
    pub(crate) fn path(&self) -> &str {
        self.home.path()
    }

    /// What the view does with `value`: a blank document or a page in the
    /// scope loads, the signal is reported, and everything else is refused.
    pub(crate) fn admit(&self, value: &str) -> Admission {
        if value == "about:blank" {
            return Admission::Load;
        }
        let Ok(url) = Url::parse(value) else {
            return Admission::Refuse;
        };
        if url.scheme() != "https"
            || url.origin() != self.home.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || !path_is_plain(url.path())
        {
            return Admission::Refuse;
        }
        let lower = url.path().to_ascii_lowercase();
        if self.signal.as_deref() == Some(lower.as_str()) {
            return Admission::Signal;
        }
        match url.path().starts_with(self.path()) && !self.is_reserved(&lower) {
            true => Admission::Load,
            false => Admission::Refuse,
        }
    }

    /// Whether a cookie may carry `path`: the prefix itself, with its final
    /// `/`, or a route under it. Without the `/`, browsers would also send
    /// it to the bare prefix, outside the scope.
    pub(crate) fn cookie_path_allowed(&self, path: &str) -> bool {
        path_is_plain(path) && path.starts_with(self.path())
    }

    /// Whether `lower` is a reserved route, the route with a format suffix
    /// (`.json`, `.html`, …), or anything under either.
    fn is_reserved(&self, lower: &str) -> bool {
        self.reserved.iter().any(|reserved| {
            lower
                .strip_prefix(reserved.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '.']))
        })
    }

    /// `route` joined to the home's path, in lower case, when it is a plain
    /// relative route.
    fn absolute(&self, route: &str) -> Result<String, InvalidScope> {
        let path = format!("{}{route}", self.path());
        let plain = !route.is_empty()
            && !route.starts_with('/')
            && !route.contains(['?', '#', '%'])
            && path_is_plain(&path);
        plain.then(|| path.to_ascii_lowercase()).ok_or(InvalidScope(
            "A session route must be a plain path under the home, such as session/required.",
        ))
    }
}

/// The rule a session's view follows: [`SessionScope::admit`], except that
/// the blank page the view starts on is refused once a page in the scope
/// has loaded, so Back cannot return to it.
pub(crate) fn view_rule(scope: SessionScope) -> impl Fn(&str) -> Admission + 'static {
    let started = Cell::new(false);
    move |value| match scope.admit(value) {
        Admission::Load if value == "about:blank" && started.get() => Admission::Refuse,
        Admission::Load if value != "about:blank" => {
            started.set(true);
            Admission::Load
        }
        admission => admission,
    }
}

/// Whether an absolute path is one route to every server: no empty or dot
/// segments, no backslash, `;` parameter or whitespace, and no
/// percent-escape of a character a server would decode into the route.
fn path_is_plain(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains("//")
        && !path.contains("..")
        && !path.split('/').any(|segment| segment == ".")
        && path
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'\\' && byte != b';')
        && escapes_are_plain(path.as_bytes())
}

/// Every `%` starts two hex digits that encode an ASCII character that is
/// neither unreserved, a separator nor a control character. Escaped
/// non-ASCII is refused too: a server that compares routes ignoring
/// Unicode case could read `%C5%BF` (ſ) as `s`.
fn escapes_are_plain(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'%')
        .all(|(at, _)| {
            bytes
                .get(at + 1..at + 3)
                .filter(|digits| digits.iter().all(u8::is_ascii_hexdigit))
                .and_then(|digits| std::str::from_utf8(digits).ok())
                .and_then(|digits| u8::from_str_radix(digits, 16).ok())
                .is_some_and(|byte| {
                    byte.is_ascii()
                        && !byte.is_ascii_alphanumeric()
                        && !HIDDEN_BYTES.contains(&byte)
                        && !byte.is_ascii_control()
                })
        })
}

#[cfg(test)]
mod tests {
    use super::{SessionScope, view_rule};
    use crate::page::Admission;

    const HOME: &str = "https://app.example.com/desktop/";

    fn scope() -> SessionScope {
        SessionScope::new(HOME)
            .and_then(|scope| scope.with_signal("session/required"))
            .and_then(|scope| scope.with_reserved("session"))
            .unwrap()
    }

    #[test]
    fn a_home_is_an_https_prefix_without_credentials_or_query() {
        assert!(SessionScope::new(HOME).is_ok());
        assert!(SessionScope::new("https://app.example.com/").is_ok());
        for refused in [
            "http://app.example.com/desktop/",
            "https://app.example.com/desktop",
            "https://alice@app.example.com/desktop/",
            "https://app.example.com/desktop/?a=1",
            "https://app.example.com/desktop/#top",
            "https://app.example.com/desktop%2f/",
            "file:///desktop/",
            "not a url",
        ] {
            assert!(SessionScope::new(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn routes_are_plain_relative_paths() {
        let home = SessionScope::new(HOME).unwrap();
        for refused in [
            "",
            "/session",
            "../api",
            "a/./b",
            "session?x=1",
            "session#x",
            "session%2frequired",
            "a b",
        ] {
            assert!(home.clone().with_signal(refused).is_err(), "{refused}");
            assert!(home.clone().with_reserved(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn only_pages_under_the_prefix_load() {
        let scope = scope();
        for loaded in [
            HOME,
            "about:blank",
            "https://app.example.com/desktop/team/overview?q=1#work",
            "https://APP.example.com:443/desktop/reports",
            "https://app.example.com/desktop/sessions",
        ] {
            assert_eq!(scope.admit(loaded), Admission::Load, "{loaded}");
        }
        for refused in [
            "https://app.example.com/",
            "https://app.example.com/desktop",
            "https://app.example.com/desktop-lookalike/api",
            "https://app.example.com/auth/login",
            "http://app.example.com/desktop/",
            "https://app.example.com:8443/desktop/",
            "https://sign-in.example.net/desktop/",
            "https://alice@app.example.com/desktop/",
            "https://alice:pw@app.example.com/desktop/",
            "about:srcdoc",
            "blob:https://app.example.com/0c5e",
            "data:text/html,<p>",
            "javascript:alert(1)",
            "not a url",
        ] {
            assert_eq!(scope.admit(refused), Admission::Refuse, "{refused}");
        }
    }

    #[test]
    fn traversal_and_encoded_separators_stay_out() {
        let scope = scope();
        for refused in [
            "https://app.example.com/desktop/../api",
            "https://app.example.com/desktop/%2e%2e/api",
            "https://app.example.com/desktop/.%2E/api",
            "https://app.example.com/desktop/..%2fapi",
            "https://app.example.com/desktop/%2E%2E%2Fapi",
            "https://app.example.com/desktop/..%5capi",
            "https://app.example.com/desktop\\..\\api",
            "https://app.example.com/desktop/%252e%252e/api",
            "https://app.example.com/desktop/..;/api",
            "https://app.example.com/desktop/a%00b",
        ] {
            assert_eq!(scope.admit(refused), Admission::Refuse, "{refused}");
        }
    }

    #[test]
    fn reserved_routes_never_load_and_the_signal_is_reported() {
        let scope = scope();
        for reserved in [
            "https://app.example.com/desktop/session",
            "https://app.example.com/desktop/session/",
            "https://app.example.com/desktop/session/redeem",
        ] {
            assert_eq!(scope.admit(reserved), Admission::Refuse, "{reserved}");
        }
        for signal in [
            "https://app.example.com/desktop/session/required",
            "https://app.example.com/desktop/session/required?from=%2Fdesktop%2F",
        ] {
            assert_eq!(scope.admit(signal), Admission::Signal, "{signal}");
        }
        assert_eq!(
            scope.admit("https://other.example.com/desktop/session/required"),
            Admission::Refuse
        );
    }

    #[test]
    fn a_reserved_route_with_a_format_suffix_never_loads() {
        let scope = scope();
        for reserved in [
            "https://app.example.com/desktop/session.json",
            "https://app.example.com/desktop/Session.HTML",
            "https://app.example.com/desktop/session.",
            "https://app.example.com/desktop/session.json/redeem",
        ] {
            assert_eq!(scope.admit(reserved), Admission::Refuse, "{reserved}");
        }
        for loaded in [
            "https://app.example.com/desktop/sessions",
            "https://app.example.com/desktop/session-notes",
            "https://app.example.com/desktop/sessionjson",
            "https://app.example.com/desktop/reports/session.json",
        ] {
            assert_eq!(scope.admit(loaded), Admission::Load, "{loaded}");
        }
    }

    #[test]
    fn reserved_routes_and_the_signal_survive_rewriting() {
        let scope = scope();
        for reserved in [
            "https://app.example.com/desktop//session",
            "https://app.example.com/desktop/session;x/redeem",
            "https://app.example.com/desktop/%73ession/redeem",
            "https://app.example.com/desktop/Session/Redeem",
            "https://app.example.com/desktop/SESSION",
        ] {
            assert_eq!(scope.admit(reserved), Admission::Refuse, "{reserved}");
        }
        assert_eq!(
            scope.admit("https://app.example.com/desktop/Session/Required"),
            Admission::Signal
        );
        for refused in [
            "https://app.example.com/desktop/%7Euser",
            "https://app.example.com/desktop/a%2",
            "https://app.example.com/desktop/a%+1b",
            "https://app.example.com/desktop/a%0a",
        ] {
            assert_eq!(scope.admit(refused), Admission::Refuse, "{refused}");
        }
        assert_eq!(
            scope.admit("https://app.example.com/desktop/a%20file"),
            Admission::Load,
            "an escaped space is not a hidden route"
        );
    }

    #[test]
    fn escaped_non_ascii_cannot_stand_in_for_a_reserved_route() {
        let scope = scope();
        for refused in [
            "https://app.example.com/desktop/%C5%BFession/redeem",
            "https://app.example.com/desktop/se%E2%84%AAsion",
            "https://app.example.com/desktop/\u{17f}ession",
            "https://app.example.com/desktop/caf%C3%A9",
        ] {
            assert_eq!(scope.admit(refused), Admission::Refuse, "{refused}");
        }
    }

    #[test]
    fn once_a_page_loads_the_blank_start_page_is_refused() {
        let rule = view_rule(scope());
        assert_eq!(
            rule("about:blank"),
            Admission::Load,
            "the view starts blank"
        );
        assert_eq!(rule("about:blank"), Admission::Load);
        assert_eq!(
            rule("https://app.example.com/desktop/reports"),
            Admission::Load
        );
        assert_eq!(
            rule("about:blank"),
            Admission::Refuse,
            "Back stops at the first page"
        );
        assert_eq!(
            rule("https://app.example.com/desktop/session/required"),
            Admission::Signal
        );
        assert_eq!(rule("https://other.example.com/"), Admission::Refuse);
    }

    #[test]
    fn a_scope_of_the_whole_site_takes_a_root_cookie_only() {
        let site = SessionScope::new("https://app.example.com/").unwrap();
        assert!(site.cookie_path_allowed("/"));
        assert!(site.cookie_path_allowed("/desktop/"));
        assert!(!site.cookie_path_allowed(""));
        assert_eq!(
            site.admit("https://app.example.com/anything"),
            Admission::Load
        );
    }

    #[test]
    fn a_scope_without_a_signal_or_reserved_routes_loads_every_route() {
        let scope = SessionScope::new(HOME).unwrap();
        assert_eq!(
            scope.admit("https://app.example.com/desktop/session/required"),
            Admission::Load
        );
    }

    #[test]
    fn a_cookie_stays_inside_the_prefix() {
        let scope = scope();
        for allowed in ["/desktop/", "/desktop/reports", "/desktop/reports/"] {
            assert!(scope.cookie_path_allowed(allowed), "{allowed}");
        }
        for refused in [
            "",
            "/",
            "/desktop",
            "/desk",
            "/desktop-lookalike/",
            "/desktop/../",
            "/desktop/%2e%2e/",
            "/desktop/;x",
            "/desktop/ x",
            "/desktop//x",
            "/desktop/%78",
        ] {
            assert!(!scope.cookie_path_allowed(refused), "{refused}");
        }
    }
}
