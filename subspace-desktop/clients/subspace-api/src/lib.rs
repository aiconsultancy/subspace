//! Blocking Subspace API or NETT reads and writes. Use the hub or a
//! background task. The product chooses the backend once; see [`backend()`].
mod activity;
pub mod backend;
pub mod chat_uploads;
pub mod decision_writes;
mod dev_sign_in;
mod fixture;
pub mod hub;
pub mod initiatives;
mod irc_credential;
pub mod issue_moves;
pub mod issue_writes;
mod list;
mod live;
pub mod media;
pub mod model;
mod native;
pub mod nett;
mod pacing;
mod query;
mod reads;
pub mod reference;
pub mod resolution_cache;
mod transport;
pub mod uploads;
mod writing;
pub use activity::ActivityQuery;
pub use backend::{
    Backend, Capabilities, backend, capabilities, configure_no_backend, has_backend,
    may_serve_initiative_channels, service_name, unreachable_message,
};
pub use dev_sign_in::{DevSignIn, DevUser};
pub use fixture::fixture_from_env;
pub use irc_credential::IrcCredential;
pub use list::{ListQuery, ListTab};
pub use native::{
    api_base, configure_api_origin, configure_nett_origin, configured_api_origin,
    signed_in_generation, use_native_account, use_token_renewal,
};
pub use query::{DecisionQuery, IssueQuery, MAX_DECISION_PAGE_SIZE, PageRequest};
use serde::de::DeserializeOwned;
use subspace_framework::core::Secret;
pub use transport::Error;
use url::Url;
pub use writing::{CommentIntent, CommentPost, WriteError};

pub const PROFILE_ID: &str = "subspace-api";

#[derive(Clone, Copy)]
struct NativeLease(Option<u64>);

/// One fixed origin; an optional development assertion is kept only in memory.
/// Deliberately has no Debug or Serialize implementation.
#[derive(Clone)]
pub struct Client {
    base: Url,
    access_jwt: Option<Secret>,
    agent: ureq::Agent,
    /// Whether a hub built on this client follows live updates.
    live: bool,
    native_lease: Option<NativeLease>,
    backend: Backend,
    /// The NETT organisation every tracker request is scoped to.
    org: Option<String>,
}

impl Client {
    pub fn new() -> Result<Self, Error> {
        // A build with no tracker backend builds no client, fixtures included.
        backend::require_backend()?;
        // Debug fixtures (capture, offline servers) pair a loopback URL with
        // a private token file. Without the file, the configured origin and
        // the signed-in account apply, as in release builds.
        #[cfg(debug_assertions)]
        {
            // A NETT build never honours the Subspace API fixture variables.
            let path = std::env::var_os("SUBSPACE_API_ACCESS_JWT_FILE")
                .filter(|_| backend::backend() == Backend::Subspace);
            let fixture = path
                .as_ref()
                .and_then(|_| std::env::var("SUBSPACE_API_URL").ok());
            fixture
                .as_deref()
                .map(|api| Self::debug_override(api, path.as_deref().map(std::path::Path::new)))
                .unwrap_or_else(Self::production)
        }
        #[cfg(not(debug_assertions))]
        {
            Self::production()
        }
    }

    fn production() -> Result<Self, Error> {
        let native_generation = native::generation();
        let backend = backend::backend();
        Ok(Self {
            base: native::origin().ok_or(Error::InvalidRequest)?.clone(),
            access_jwt: None,
            live: true,
            agent: native::agent(native_generation),
            native_lease: Some(NativeLease(native_generation)),
            backend,
            org: (backend == Backend::Nett)
                .then(|| nett::scope::selected(native::account_generation()))
                .flatten(),
        })
    }

    /// Explicit loopback injection for offline tests.
    // qual:test_helper
    #[cfg(any(debug_assertions, feature = "test-support"))]
    pub fn loopback(api: &str, jwt: Option<Secret>) -> Result<Self, Error> {
        let base = fixture::loopback_url(api)?;
        Ok(Self {
            agent: transport::agent_for(&base),
            base,
            access_jwt: cfg!(debug_assertions).then_some(jwt).flatten(),
            live: true,
            native_lease: None,
            backend: Backend::Subspace,
            org: None,
        })
    }

    /// Explicit loopback NETT client for offline tests, scoped to `org`.
    // qual:test_helper
    #[cfg(any(debug_assertions, feature = "test-support"))]
    pub fn nett_loopback(
        api: &str,
        org: Option<&str>,
        token: Option<Secret>,
    ) -> Result<Self, Error> {
        let mut client = Self::loopback(api, token)?;
        client.backend = Backend::Nett;
        client.org = org.map(str::to_owned);
        client.agent = transport::agent_with_flow(&client.base, Some(client.account_key()));
        Ok(client)
    }

    #[cfg(debug_assertions)]
    fn debug_override(api: &str, path: Option<&std::path::Path>) -> Result<Self, Error> {
        // Validate before reading the assertion: only explicit loopback can use it.
        let base = fixture::loopback_url(api)?;
        let jwt = fixture::read_access_jwt(path)?;
        Ok(Self {
            agent: transport::agent_for(&base),
            base,
            access_jwt: jwt,
            live: true,
            native_lease: None,
            backend: Backend::Subspace,
            org: None,
        })
    }

    /// Disables the issue update stream, for tests that
    /// count every request.
    // qual:test_helper
    #[cfg(any(test, feature = "test-support"))]
    pub fn without_live(mut self) -> Self {
        self.live = false;
        self
    }

    pub fn api_origin(&self) -> &str {
        self.base.as_str().trim_end_matches('/')
    }
    /// Whether this client's backend serves `feature` for its organisation.
    /// NETT's capability list is read the first time it is needed and again
    /// once it is due; if it was never read and cannot be, the feature
    /// counts as not served.
    pub(crate) fn serves(&self, feature: backend::Feature) -> bool {
        match self.backend {
            Backend::Subspace => true,
            Backend::Nett => self.nett_serves(feature).unwrap_or(false),
        }
    }
    /// The service this client talks to.
    pub fn service(&self) -> Backend {
        self.backend
    }
    /// The NETT organisation this client is scoped to.
    pub fn organisation(&self) -> Option<&str> {
        self.org.as_deref()
    }
    /// Which account this client acts for: its native lease, or for an
    /// explicit loopback client a digest of its token. Caches of private
    /// answers, such as file grants, are kept apart by it.
    pub(crate) fn account_key(&self) -> u64 {
        if let Some(NativeLease(Some(generation))) = self.native_lease {
            return generation;
        }
        self.access_jwt.as_ref().map_or(0, |jwt| {
            use sha2::Digest as _;
            let digest = sha2::Sha256::digest(jwt.expose().as_bytes());
            u64::from_be_bytes(digest[..8].try_into().unwrap_or_default())
        })
    }
    /// The account NETT's flow control counts this client's requests under:
    /// the native account, whatever the organisation, or for an explicit
    /// loopback client its token.
    pub(crate) fn flow_account(&self) -> u64 {
        if self.native_lease.is_some() {
            return native::account_generation().unwrap_or_default();
        }
        self.account_key()
    }
    /// Whether this client's account or organisation scope is still current.
    pub(crate) fn current(&self) -> Result<(), Error> {
        if self.account_changed() {
            Err(Error::SignInNeeded)
        } else {
            Ok(())
        }
    }
    /// Whether this client belongs to a superseded native account.
    pub fn account_changed(&self) -> bool {
        self.native_lease
            .is_some_and(|NativeLease(generation)| generation != native::generation())
    }
    fn get<T: DeserializeOwned>(
        &self,
        path: &[&str],
        query: &[(String, String)],
    ) -> Result<T, Error> {
        if self.backend == Backend::Nett {
            return self.nett_get(path, query);
        }
        let url = transport::endpoint(&self.base, path, query)?;
        transport::get(&self.agent, &url, self.access_jwt.as_ref())
    }

    /// A tracker route's URL: as given for Subspace API, inside the selected
    /// organisation for NETT.
    pub(crate) fn endpoint(&self, path: &[&str], query: &[(String, String)]) -> Result<Url, Error> {
        match self.backend {
            Backend::Subspace => transport::endpoint(&self.base, path, query),
            Backend::Nett => {
                let org = self.org.as_deref().ok_or(Error::OrganisationNeeded)?;
                let mut scoped = vec!["orgs", org];
                scoped.extend(path);
                transport::endpoint(&self.base, &scoped, &nett::query::translate(path, query))
            }
        }
    }

    /// Reads a JSON reply as the desktop's model, mapping NETT's shapes.
    pub(crate) fn read<T: DeserializeOwned>(
        &self,
        response: ureq::http::Response<ureq::Body>,
    ) -> Result<T, Error> {
        if self.backend == Backend::Subspace {
            return transport::read_json(response);
        }
        let mut value: serde_json::Value = transport::read_json(response)?;
        // A reply read after the account or organisation changed belongs to
        // no one.
        self.current()?;
        nett::wire::normalise(&mut value);
        serde_json::from_value(value).map_err(|_| Error::InvalidResponse)
    }
}

/// The profile's name, as tests name a Subspace API hub.
#[cfg(test)]
pub(crate) const PROFILE_NAME: &str = "Subspace API";

#[cfg(test)]
mod tests;
