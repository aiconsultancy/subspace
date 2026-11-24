//! Where the account's side of Channels reaches the Subspace API and learns
//! which account is signed in. Tests stand in for both.

use subspace_api_client::{Client, Error};

pub(crate) trait Env: Send + Sync {
    /// A client for the account signed in now. Makes no request.
    fn client(&self) -> Result<Client, Error>;
    /// The signed-in account's generation, which changes with every
    /// sign-in, sign-out and switch.
    fn generation(&self) -> Option<u64>;
    /// Whether the build has an API at all. A build with no API backend has
    /// none, so the account brings no network and nothing asks it for one.
    fn available(&self) -> bool {
        true
    }
}

/// The app's own account and API.
pub(crate) struct Native;

impl Env for Native {
    fn client(&self) -> Result<Client, Error> {
        Client::new()
    }

    fn generation(&self) -> Option<u64> {
        subspace_api_client::signed_in_generation()
    }

    fn available(&self) -> bool {
        subspace_api_client::has_backend()
    }
}
