//! Where a tracker write goes: the Issues Space's scope names the tracker
//! (its profile and API origin) the commands' and dialogs' choices came
//! from, and Channels writes through its own API client. A write is sent
//! only when that client talks to the tracker the scope names, as an
//! account that is still current; otherwise nothing is sent. The scope
//! carries no account of its own: both sides take the desktop's one
//! signed-in Subspace API account, which the client's lease checks.

use std::sync::Arc;

use subspace_api_client::{Backend, Client, PROFILE_ID};
use subspace_framework::contracts::issues::IssueScope;

/// Why nothing could be written without a client.
pub(crate) const NO_CLIENT: &str =
    "Couldn't reach Subspace API. Check Settings › Account, then try again.";
/// Why nothing was written to a client that is not the scope's tracker.
pub(crate) const WRONG_TRACKER: &str = "Channels' Subspace API connection isn't the tracker the Issues Space shows, so nothing was written. Check Settings › Account, then try again.";

/// `client`, when it writes to the tracker `scope` names; otherwise why
/// nothing may be written.
pub(crate) fn matching(
    client: Option<Arc<Client>>,
    scope: Option<&IssueScope>,
) -> Result<Arc<Client>, String> {
    let client = client.ok_or_else(|| NO_CLIENT.to_owned())?;
    let scope = scope.ok_or_else(|| super::NO_TRACKER.to_owned())?;
    if names(&client, scope) {
        Ok(client)
    } else {
        Err(WRONG_TRACKER.to_owned())
    }
}

/// Whether `scope` names `client`'s tracker: Subspace API, at the same
/// origin, for an account that is still current.
fn names(client: &Client, scope: &IssueScope) -> bool {
    client.service() == Backend::Subspace
        && scope.profile_id == PROFILE_ID
        && same_origin(client.api_origin(), &scope.api_origin)
        && !client.account_changed()
}

/// Whether two API bases are the same: scheme, host and port compared as
/// URLs compare them (without case, the scheme's default port filled in),
/// and the base path compared exactly, as a server may route paths that
/// differ only in case to different trackers.
fn same_origin(one: &str, other: &str) -> bool {
    match (base(one), base(other)) {
        (Some(one), Some(other)) => one == other,
        _ => false,
    }
}

/// The default ports a base URL leaves out.
const HTTP_PORT: u16 = 80;
const HTTPS_PORT: u16 = 443;

/// A base URL's scheme, host, port and path, normalised.
fn base(url: &str) -> Option<(String, String, u16, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.contains(']') => (host, Some(port.parse().ok()?)),
        _ => (authority, None),
    };
    let port = port.or(match scheme.as_str() {
        "http" => Some(HTTP_PORT),
        "https" => Some(HTTPS_PORT),
        _ => None,
    })?;
    let path = path.trim_end_matches('/').to_owned();
    Some((scheme, host.to_ascii_lowercase(), port, path))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use subspace_api_client::{Client, PROFILE_ID};
    use subspace_framework::contracts::issues::IssueScope;

    use super::{NO_CLIENT, WRONG_TRACKER, matching};

    fn client(api: &str) -> Option<Arc<Client>> {
        Some(Arc::new(
            Client::loopback(api, None).unwrap().without_live(),
        ))
    }

    #[test]
    fn a_write_goes_only_to_the_tracker_the_scope_names() {
        let scope = IssueScope::new(PROFILE_ID, "Subspace API", "http://127.0.0.1:9/api/");
        assert!(matching(client("http://127.0.0.1:9/api"), Some(&scope)).is_ok());
        assert_eq!(
            matching(client("http://127.0.0.1:10/api"), Some(&scope)).err(),
            Some(WRONG_TRACKER.to_owned()),
            "same project keys, another tracker"
        );
        let other = IssueScope::new("nett", "NETT", "http://127.0.0.1:9/api");
        assert!(matching(client("http://127.0.0.1:9/api"), Some(&other)).is_err());
        assert_eq!(
            matching(None, Some(&scope)).err(),
            Some(NO_CLIENT.to_owned())
        );
        assert!(matching(client("http://127.0.0.1:9/api"), None).is_err());
    }

    #[test]
    fn hosts_compare_as_urls_do_and_paths_exactly() {
        use super::same_origin;
        assert!(same_origin(
            "HTTP://Tracker.Example.com/api",
            "http://tracker.example.com:80/api/"
        ));
        assert!(same_origin(
            "https://tracker.example.com/api",
            "https://TRACKER.example.com:443/api"
        ));
        assert!(!same_origin(
            "https://tracker.example.com/TeamA/api",
            "https://tracker.example.com/teama/api"
        ));
        assert!(!same_origin(
            "https://tracker.example.com/api",
            "http://tracker.example.com/api"
        ));
        assert!(!same_origin(
            "https://tracker.example.com:8443/api",
            "https://tracker.example.com/api"
        ));
        assert!(!same_origin("not a url", "not a url"));
    }
}
