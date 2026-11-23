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

fn same_origin(one: &str, other: &str) -> bool {
    one.trim_end_matches('/')
        .eq_ignore_ascii_case(other.trim_end_matches('/'))
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
}
