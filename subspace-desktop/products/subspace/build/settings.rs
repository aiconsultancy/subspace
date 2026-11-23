//! Rules shared by the build script and the app, so a build setting and its
//! run-time override are checked the same way: service URLs, the WorkOS
//! client ID, and which settings belong to which tracker backend.

/// The tracker backend a build talks to, or `None` for a build with no API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApiBackend {
    Subspace,
    Nett,
    /// No API and no sign-in: Channels, the GitHub Spaces and web Spaces
    /// only, configured by hand.
    None,
}

impl ApiBackend {
    /// `api_backend` from the build configuration; empty is Subspace API.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "" | "subspace" => Some(Self::Subspace),
            "nett" => Some(Self::Nett),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// An API base URL: an absolute HTTPS origin, or plain HTTP on a loopback
/// address for local services and fixtures, optionally followed by `/api`.
/// No credentials, other paths, query, fragment or control characters.
pub(crate) fn valid_api_url(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name == "localhost",
        None => false,
    };
    (url.scheme() == "https" || (url.scheme() == "http" && loopback))
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "" | "/" | "/api" | "/api/")
        && url.query().is_none()
        && url.fragment().is_none()
}

/// The longest client ID the WorkOS dashboard shows, prefix included.
const WORKOS_CLIENT_ID_LIMIT: usize = 128;

/// A WorkOS client ID as the dashboard shows it: `client_` followed by
/// ASCII letters and digits. Subspace API checks `WORKOS_CLIENT_ID` the
/// same way.
pub(crate) fn valid_workos_client_id(value: &str) -> bool {
    value.strip_prefix("client_").is_some_and(|rest| {
        !rest.is_empty()
            && value.len() <= WORKOS_CLIENT_ID_LIMIT
            && rest.bytes().all(|c| c.is_ascii_alphanumeric())
    })
}

/// Run-time variables only a Subspace API build uses. In a NETT build a
/// non-empty value is a configuration error, so nothing can point the app
/// back at Subspace API or its development sign-in.
pub(crate) const SUBSPACE_ONLY_VARIABLES: [&str; 3] = [
    "SUBSPACE_API_URL",
    "SUBSPACE_API_ACCESS_JWT_FILE",
    "SUBSPACE_WORKOS_REDIRECT_PORT",
];

/// Prefixes of the other Subspace-only variables.
pub(crate) const SUBSPACE_ONLY_PREFIXES: [&str; 2] = ["SUBSPACE_DEV_", "SUBSPACE_ENTRA_"];

/// Run-time variables, besides the Subspace-only ones, that only a build
/// with an API backend uses. In a build with no API backend every one of
/// them set to a non-empty value is a configuration error, as in a NETT
/// build, so nothing can give it an API or a sign-in.
pub(crate) const BACKEND_VARIABLES: [&str; 2] =
    ["SUBSPACE_NETT_API_URL", "SUBSPACE_IDENTITY_PROVIDER"];

/// Prefixes of the other variables only a build with an API backend uses.
pub(crate) const BACKEND_PREFIXES: [&str; 1] = ["SUBSPACE_WORKOS_"];

/// The Subspace-only variables set to a non-empty value, from `(name,
/// value)` pairs such as `std::env::vars()`.
pub(crate) fn refused_in_nett<'a>(
    variables: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    set_among(variables, &SUBSPACE_ONLY_VARIABLES, &SUBSPACE_ONLY_PREFIXES)
}

/// Every API and sign-in variable set to a non-empty value: what a build
/// with no API backend refuses.
pub(crate) fn refused_without_backend<'a>(
    variables: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    let names = [SUBSPACE_ONLY_VARIABLES.as_slice(), &BACKEND_VARIABLES].concat();
    let prefixes = [SUBSPACE_ONLY_PREFIXES.as_slice(), &BACKEND_PREFIXES].concat();
    set_among(variables, &names, &prefixes)
}

/// The variables named in `names`, or starting with one of `prefixes`, set
/// to a non-empty value, sorted.
fn set_among<'a>(
    variables: impl IntoIterator<Item = (&'a str, &'a str)>,
    names: &[&str],
    prefixes: &[&str],
) -> Vec<String> {
    let mut refused: Vec<String> = variables
        .into_iter()
        .filter(|(name, value)| {
            !value.trim().is_empty()
                && (names.contains(name) || prefixes.iter().any(|prefix| name.starts_with(prefix)))
        })
        .map(|(name, _)| name.to_owned())
        .collect();
    refused.sort();
    refused
}

#[cfg(test)]
mod tests {
    use super::{
        ApiBackend, refused_in_nett, refused_without_backend, valid_api_url, valid_workos_client_id,
    };

    #[test]
    fn api_urls_are_https_origins_or_loopback_http() {
        for (url, valid) in [
            ("https://api.example.com", true),
            ("https://api.example.com/api", true),
            ("https://api.example.com/api/", true),
            ("http://127.0.0.1:3000", true),
            ("http://localhost/api", true),
            ("http://[::1]:8080", true),
            ("", false),
            ("http://api.example.com", false),
            ("http://127.0.0.1.example.com", false),
            ("https://api.example.com/other", false),
            ("https://user@api.example.com", false),
            ("https://api.example.com/api?x=1", false),
            ("https://api.example.com/#x", false),
            ("https://api.example.com/\napi", false),
        ] {
            assert_eq!(valid_api_url(url), valid, "{url:?}");
        }
    }

    #[test]
    fn workos_client_ids_have_the_dashboard_form() {
        assert!(valid_workos_client_id("client_00000000000000000000000000"));
        assert!(valid_workos_client_id("client_x"));
        for value in [
            "",
            "client_",
            "client id",
            "client_a-b",
            "other_123",
            "client_ok\n",
        ] {
            assert!(!valid_workos_client_id(value), "{value:?}");
        }
        assert!(!valid_workos_client_id(&format!(
            "client_{}",
            "a".repeat(122)
        )));
    }

    #[test]
    fn nett_builds_refuse_every_non_empty_subspace_only_variable() {
        let refused = refused_in_nett([
            ("SUBSPACE_API_URL", "http://127.0.0.1:3000"),
            ("SUBSPACE_DEV_EMAIL", "developer@example.com"),
            ("SUBSPACE_ENTRA_TENANT_ID", "x"),
            ("SUBSPACE_API_ACCESS_JWT_FILE", ""),
            ("SUBSPACE_NETT_API_URL", "https://api.nett.example"),
            ("SUBSPACE_WORKOS_CLIENT_ID", "client_x"),
            ("SUBSPACE_WORKOS_REDIRECT_PORT", "8787"),
        ]);
        assert_eq!(
            refused,
            [
                "SUBSPACE_API_URL",
                "SUBSPACE_DEV_EMAIL",
                "SUBSPACE_ENTRA_TENANT_ID",
                "SUBSPACE_WORKOS_REDIRECT_PORT"
            ]
        );
        assert_eq!(ApiBackend::parse(""), Some(ApiBackend::Subspace));
        assert_eq!(ApiBackend::parse("nett"), Some(ApiBackend::Nett));
        assert_eq!(ApiBackend::parse("other"), None);
    }

    #[test]
    fn builds_without_a_backend_refuse_every_non_empty_api_or_sign_in_variable() {
        assert_eq!(ApiBackend::parse(" none "), Some(ApiBackend::None));
        let refused = refused_without_backend([
            ("SUBSPACE_API_URL", "http://127.0.0.1:3000"),
            ("SUBSPACE_API_ACCESS_JWT_FILE", "/tmp/fixture-token"),
            ("SUBSPACE_NETT_API_URL", "https://api.nett.example"),
            ("SUBSPACE_IDENTITY_PROVIDER", "dev"),
            ("SUBSPACE_WORKOS_CLIENT_ID", "client_x"),
            ("SUBSPACE_WORKOS_API_URL", "https://auth.example.com"),
            ("SUBSPACE_DEV_EMAIL", "developer@example.com"),
            ("SUBSPACE_ENTRA_CLIENT_ID", "x"),
            // Empty is unset, and other Subspace variables are not API settings.
            ("SUBSPACE_WORKOS_REDIRECT_PORT", " "),
            ("SUBSPACE_GITHUB_API_URL", "http://127.0.0.1:9000"),
            ("SUBSPACE_RELEASE_CHANNEL", "nightly"),
        ]);
        assert_eq!(
            refused,
            [
                "SUBSPACE_API_ACCESS_JWT_FILE",
                "SUBSPACE_API_URL",
                "SUBSPACE_DEV_EMAIL",
                "SUBSPACE_ENTRA_CLIENT_ID",
                "SUBSPACE_IDENTITY_PROVIDER",
                "SUBSPACE_NETT_API_URL",
                "SUBSPACE_WORKOS_API_URL",
                "SUBSPACE_WORKOS_CLIENT_ID",
            ]
        );
    }
}
