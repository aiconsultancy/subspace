//! The OAuth App and the GitHub addresses a sign-in talks to.

use std::net::IpAddr;

use subspace_core::Secret;

/// The scopes a sign-in asks for: gh's defaults without `gist`.
pub const SCOPES: &str = "repo read:org";
pub const OAUTH_URL: &str = "https://github.com";
pub const API_URL: &str = "https://api.github.com";
/// Debug builds only: a numeric loopback address that stands in for
/// [`OAUTH_URL`], such as `scripts/github_test_server.py`.
pub const OAUTH_URL_ENV: &str = "SUBSPACE_GITHUB_OAUTH_URL";
/// Debug builds only: a numeric loopback address that stands in for
/// [`API_URL`], shared with the GitHub client.
pub const API_URL_ENV: &str = "SUBSPACE_GITHUB_API_URL";

/// The app's OAuth App, which the app supplies (the Framework has none).
/// Without a secret, only the device flow works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClient {
    pub id: String,
    pub secret: Option<Secret>,
}

impl OAuthClient {
    /// The OAuth App with client id `id` and, if set, its secret, both
    /// trimmed. A blank id is no OAuth App; a blank secret is none.
    pub fn new(id: &str, secret: Option<&str>) -> Option<Self> {
        let id = id.trim();
        (!id.is_empty()).then(|| Self {
            id: id.to_owned(),
            secret: secret
                .map(str::trim)
                .filter(|secret| !secret.is_empty())
                .map(Secret::new),
        })
    }
}

/// Where the OAuth endpoints (`/login/...`) and the REST API live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub oauth: String,
    pub api: String,
}

impl Endpoints {
    pub fn github() -> Self {
        Self {
            oauth: OAUTH_URL.to_owned(),
            api: API_URL.to_owned(),
        }
    }

    /// GitHub, unless a debug build names loopback fixtures.
    pub fn from_build() -> Self {
        let read = |name| std::env::var(name).ok();
        Self::with_overrides(
            cfg!(debug_assertions),
            read(OAUTH_URL_ENV).as_deref(),
            read(API_URL_ENV).as_deref(),
        )
    }

    /// GitHub, with each override that is a numeric loopback HTTP address
    /// when `debug` is set.
    pub fn with_overrides(debug: bool, oauth: Option<&str>, api: Option<&str>) -> Self {
        let github = Self::github();
        let pick = |value: Option<&str>, default: String| {
            value
                .filter(|_| debug)
                .and_then(loopback_base)
                .unwrap_or(default)
        };
        Self {
            oauth: pick(oauth, github.oauth),
            api: pick(api, github.api),
        }
    }
}

/// `value` without a trailing slash, when it is `http://<loopback IP>:port`.
fn loopback_base(value: &str) -> Option<String> {
    let url = url::Url::parse(value.trim()).ok()?;
    let loopback = match url.host()? {
        url::Host::Ipv4(address) => IpAddr::V4(address).is_loopback(),
        url::Host::Ipv6(address) => IpAddr::V6(address).is_loopback(),
        url::Host::Domain(_) => false,
    };
    (url.scheme() == "http" && loopback && url.query().is_none())
        .then(|| value.trim().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use subspace_core::Secret;

    use super::{API_URL, Endpoints, OAUTH_URL, OAuthClient};

    #[test]
    fn a_blank_client_id_is_no_oauth_app_and_a_blank_secret_is_none() {
        assert_eq!(OAuthClient::new(" ", Some("secret")), None);
        let client = OAuthClient::new(" Iv1.example ", Some(" ")).unwrap();
        assert_eq!(client.id, "Iv1.example");
        assert_eq!(client.secret, None);
        let client = OAuthClient::new("Iv1.example", Some(" fixture-secret\n")).unwrap();
        assert_eq!(client.secret, Some(Secret::new("fixture-secret")));
    }

    #[test]
    fn only_a_debug_build_accepts_numeric_loopback_overrides() {
        let fixture = Endpoints::with_overrides(
            true,
            Some("http://127.0.0.1:4100/"),
            Some("http://[::1]:4101"),
        );
        assert_eq!(fixture.oauth, "http://127.0.0.1:4100");
        assert_eq!(fixture.api, "http://[::1]:4101");
        let release = Endpoints::with_overrides(false, Some("http://127.0.0.1:4100"), None);
        assert_eq!(release.oauth, OAUTH_URL);
        for refused in [
            "http://localhost:4100",
            "https://127.0.0.1:4100",
            "http://10.0.0.1:4100",
            "http://example.com",
            "not a url",
        ] {
            let endpoints = Endpoints::with_overrides(true, Some(refused), Some(refused));
            assert_eq!(endpoints.oauth, OAUTH_URL, "{refused}");
            assert_eq!(endpoints.api, API_URL, "{refused}");
        }
    }
}
