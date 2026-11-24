//! What a product's native adapter hands a web Space with a native session:
//! the site-issued cookie, how long it lasts and how to revoke it, or why
//! it could not get one.

use std::fmt;
use std::time::{Duration, Instant, SystemTime};

use subspace_framework::core::Secret;

use super::SessionScope;
use super::revocation::{REVOCATIONS, Revocation};
use crate::page::SiteCookie;

/// The most a cookie's name and value may hold together, as browsers allow.
const MAX_COOKIE_BYTES: usize = 4096;
/// The longest a grant lasts, whatever lifetime the adapter gives it.
pub const MAX_SESSION_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
/// The shortest grant the Space accepts: twice the time it renews before
/// the end, so a renewal never follows another at once.
pub const MIN_SESSION_LIFETIME: Duration = Duration::from_secs(4 * 60);

/// A moment on both clocks: the monotonic one, and the wall clock, which
/// keeps counting while the computer sleeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Now {
    pub instant: Instant,
    pub wall: SystemTime,
}

impl Now {
    pub(crate) fn current() -> Self {
        Self {
            instant: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    /// `duration` later, on both clocks, or now if that cannot be told.
    pub(crate) fn after(self, duration: Duration) -> Deadline {
        Deadline {
            instant: self.instant.checked_add(duration).unwrap_or(self.instant),
            wall: self.wall.checked_add(duration).unwrap_or(self.wall),
        }
    }
}

/// When something is due, by either clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Deadline {
    instant: Instant,
    wall: SystemTime,
}

impl Deadline {
    pub(crate) fn passed(&self, now: Now) -> bool {
        now.instant >= self.instant || now.wall >= self.wall
    }

    /// How long is left, by the monotonic clock.
    pub(crate) fn left(&self, now: Now) -> Duration {
        self.instant.saturating_duration_since(now.instant)
    }

    /// `duration` earlier, but not before `now`.
    pub(crate) fn before(&self, duration: Duration, now: Now) -> Deadline {
        Deadline {
            instant: self
                .instant
                .checked_sub(duration)
                .unwrap_or(now.instant)
                .max(now.instant),
            wall: self
                .wall
                .checked_sub(duration)
                .unwrap_or(now.wall)
                .max(now.wall),
        }
    }

    pub(crate) fn wall(&self) -> SystemTime {
        self.wall
    }
}

/// The site's session cookie: its name, its opaque value and, optionally, a
/// path narrower than the scope's. The Space sets every other attribute
/// itself: host-only on the scope's host, `Secure`, `HttpOnly`,
/// `SameSite=Strict`, and expiring with its grant.
#[derive(Clone)]
pub struct SessionCookie {
    name: String,
    value: Secret,
    path: Option<String>,
}

impl SessionCookie {
    pub fn new(name: impl Into<String>, value: Secret) -> Self {
        Self {
            name: name.into(),
            value,
            path: None,
        }
    }

    /// The cookie's path: the scope's path or a route under it. Without
    /// one, the cookie has the scope's path.
    // qual:api — a product adapter narrows its cookie; the tests cover the check.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

impl fmt::Debug for SessionCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionCookie")
            .field("name", &self.name)
            .field("value", &self.value)
            .field("path", &self.path)
            .finish()
    }
}

/// Why an attempt failed, in words for the person, and whether the Space
/// may try again by itself. Never put a credential in the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFailure {
    pub(super) message: String,
    pub(super) transient: bool,
}

impl SessionFailure {
    /// The person must act, then choose Retry.
    pub fn action(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            transient: false,
        }
    }

    /// A passing problem, such as the network: a renewal tries again by
    /// itself, a few times.
    pub fn temporary(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            transient: true,
        }
    }
}

/// Why a grant's cookie cannot go in the view, and whose mistake it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CookieProblem {
    /// The product named the cookie or narrowed its path wrongly.
    Product(&'static str),
    /// The site returned an unusable value.
    Site,
}

/// A site-issued cookie the adapter validated, and how long it lasts. Each
/// grant must be its own session at the site: a renewal's grant replaces
/// the last, which is then revoked. With [`SessionGrant::with_revoke`], the
/// session is revoked at the site once, off the UI thread, when the Space
/// lets the grant go or the app quits.
pub struct SessionGrant {
    cookie: SessionCookie,
    lifetime: Duration,
    pub(super) ends: Deadline,
    revocation: Option<Revocation>,
}

impl SessionGrant {
    /// `cookie`, good for `lifetime` from now, at most
    /// [`MAX_SESSION_LIFETIME`]. The Space refuses a grant shorter than
    /// [`MIN_SESSION_LIFETIME`], renews it before it ends, and the view
    /// stops sending it when it ends.
    pub fn new(cookie: SessionCookie, lifetime: Duration) -> Self {
        Self::starting(cookie, lifetime, Now::current())
    }

    pub(super) fn starting(cookie: SessionCookie, lifetime: Duration, now: Now) -> Self {
        let lifetime = lifetime.min(MAX_SESSION_LIFETIME);
        Self {
            cookie,
            lifetime,
            ends: now.after(lifetime),
            revocation: None,
        }
    }

    /// Ends the session at the site, best effort and bounded by the
    /// adapter's own timeout. It runs once, on its own thread. Calling it
    /// again replaces the earlier revocation, which then never runs.
    // qual:api — a product adapter's own HTTP call; the tests cover when it runs.
    pub fn with_revoke(mut self, revoke: impl FnOnce() + Send + 'static) -> Self {
        match &self.revocation {
            Some(revocation) => revocation.replace(Box::new(revoke)),
            None => self.revocation = Some(REVOCATIONS.register(Box::new(revoke))),
        }
        self
    }

    /// The cookie as the view's store holds it, when it is a valid cookie
    /// for `scope`: a token name, an opaque value, a path in the scope, and
    /// a grant at least [`MIN_SESSION_LIFETIME`] long.
    pub(super) fn site_cookie(&self, scope: &SessionScope) -> Result<SiteCookie, CookieProblem> {
        let cookie = &self.cookie;
        let path = cookie.path.as_deref().unwrap_or(scope.path());
        let value = cookie.value.expose();
        if cookie.name.is_empty() || !cookie.name.bytes().all(is_token) {
            return Err(CookieProblem::Product(
                "The session cookie's name is not a token.",
            ));
        }
        if !scope.cookie_path_allowed(path) {
            return Err(CookieProblem::Product(
                "The session cookie's path is outside the session's scope.",
            ));
        }
        let valid = self.lifetime >= MIN_SESSION_LIFETIME
            && !value.is_empty()
            && value.bytes().all(is_cookie_octet)
            && cookie.name.len() + value.len() <= MAX_COOKIE_BYTES;
        valid
            .then(|| SiteCookie {
                name: cookie.name.clone(),
                value: cookie.value.clone(),
                domain: scope.host().to_owned(),
                path: path.to_owned(),
                expires: self.ends.wall(),
            })
            .ok_or(CookieProblem::Site)
    }
}

impl fmt::Debug for SessionGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionGrant")
            .field("cookie", &self.cookie)
            .field("revoke", &self.revocation.is_some())
            .finish_non_exhaustive()
    }
}

/// An HTTP token character, as a cookie name needs.
fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// A cookie value character: visible ASCII less `"`, `,`, `;` and `\`.
fn is_cookie_octet(byte: u8) -> bool {
    byte.is_ascii_graphic() && !b"\",;\\".contains(&byte)
}
