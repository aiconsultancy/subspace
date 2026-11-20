//! Blocking Subspace API or NETT reads and writes. Use the hub or a
//! background task. The product chooses the backend once; see [`backend`].
mod activity;
pub mod backend;
pub mod chat_uploads;
mod dev_sign_in;
mod fixture;
pub mod hub;
pub mod initiatives;
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
pub mod reference;
pub mod resolution_cache;
mod transport;
pub mod uploads;
mod writing;
pub use activity::ActivityQuery;
pub use backend::{Backend, Capabilities, backend, capabilities, service_name};
pub use dev_sign_in::{DevSignIn, DevUser};
pub use fixture::fixture_from_env;
pub use list::{ListQuery, ListTab};
use model::{
    Comment, IssueDetail, IssueGroups, IssueListItem, Me, Page, Project, Relation, WorkflowState,
};
pub use native::{
    api_base, configure_api_origin, configure_nett_origin, configured_api_origin,
    use_native_account, use_token_renewal,
};
pub use query::{DecisionQuery, IssueQuery, MAX_DECISION_PAGE_SIZE, PageRequest};
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use subspace_framework::core::Secret;
pub use transport::Error;
use url::Url;
pub use writing::{CommentIntent, CommentPost, WriteError};

pub const PROFILE_ID: &str = "subspace-api";
pub const PROFILE_NAME: &str = "Subspace API";

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
    /// NETT's capability list is read the first time it is needed; if it
    /// cannot be read, the feature counts as not served.
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
    pub fn me(&self) -> Result<Me, Error> {
        if self.backend == Backend::Nett {
            return self.nett_me();
        }
        self.get(&["me"], &[])
    }

    /// Initiative memberships of the current Subspace API caller. An item
    /// that names only its fleet's ID gets the fleet read separately. If that
    /// read fails, the item keeps its `fleet_id` with no `fleet`: callers must
    /// treat it as unresolved, never as the default server, and the shared
    /// initiative catalogue reads it again with backoff.
    pub fn member_initiatives(&self) -> Result<Vec<model::MemberInitiative>, Error> {
        if self.backend == Backend::Nett
            && !self.nett_serves(backend::Feature::InitiativeChannels)?
        {
            return self.nett_initiatives();
        }
        let mut items: Vec<model::MemberInitiative> = if self.backend == Backend::Nett {
            // NETT pages its initiatives with channels; Subspace API returns them all.
            self.nett_all(&["me", "initiatives"], &[])?
                .into_iter()
                .map(|mut item| {
                    nett::wire::label_initiative(&mut item);
                    serde_json::from_value(item)
                })
                .collect::<Result<_, _>>()
                .map_err(|_| Error::InvalidResponse)?
        } else {
            let page: Page<model::MemberInitiative> = self.get(&["me", "initiatives"], &[])?;
            page.items
        };
        for item in items.iter_mut().filter(|item| item.fleet.is_none()) {
            if let Some(id) = item.fleet_id.clone() {
                item.fleet = self.fleet(&id).ok();
            }
        }
        Ok(items)
    }

    /// One fleet by ID or slug.
    pub fn fleet(&self, id: &str) -> Result<model::Fleet, Error> {
        if !self.serves(backend::Feature::InitiativeChannels) {
            return Err(Error::NotFound);
        }
        self.get(&["fleets", id], &[])
    }

    pub fn projects(&self, page: &PageRequest) -> Result<Page<Project>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_teams(page);
        }
        let mut parameters = page.parameters(query::MAX_LIST_PAGE_SIZE)?;
        parameters.push(("includeArchived".into(), "true".into()));
        self.get(&["projects"], &parameters)
    }

    /// Read all project pages and reject a cursor that repeats.
    pub fn all_projects(&self) -> Result<Vec<Project>, Error> {
        let mut request = PageRequest {
            limit: query::MAX_LIST_PAGE_SIZE,
            cursor: None,
        };
        let mut projects = vec![];
        let mut cursors = HashSet::new();
        loop {
            let page = self.projects(&request)?;
            projects.extend(page.items);
            let Some(cursor) = page.next_cursor else {
                return Ok(projects);
            };
            if !cursors.insert(cursor.clone()) {
                return Err(Error::InvalidResponse);
            }
            request.cursor = Some(cursor);
        }
    }

    pub fn workflow_states(
        &self,
        project: &str,
        page: &PageRequest,
    ) -> Result<Page<WorkflowState>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_workflow_states(project);
        }
        self.get(
            &["projects", project, "workflow-states"],
            &page.parameters(query::MAX_LIST_PAGE_SIZE)?,
        )
    }

    pub fn issue_groups(&self, query: &ListQuery) -> Result<IssueGroups, Error> {
        self.get(&["issues", "groups"], &query.grouped_parameters()?)
    }

    fn count_groups(&self, parameters: &[(String, String)]) -> Result<IssueGroups, Error> {
        self.get(&["issues", "groups"], parameters)
    }

    pub fn issue_group_page(
        &self,
        query: &ListQuery,
        group: &model::IssueGroup,
        cursor: Option<&str>,
    ) -> Result<Page<IssueListItem>, Error> {
        self.get(&["issues"], &query.continuation_parameters(group, cursor)?)
    }

    pub fn issues(&self, query: &IssueQuery) -> Result<Page<IssueListItem>, Error> {
        self.get(&["issues"], &query.parameters()?)
    }

    pub fn issue(&self, reference: &str) -> Result<IssueDetail, Error> {
        self.get(&["issues", reference], &[])
    }

    pub fn decisions(
        &self,
        query: &DecisionQuery,
        page: &PageRequest,
    ) -> Result<Page<model::DecisionListItem>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_decisions(query, page);
        }
        self.get(&["decisions"], &query.parameters(page)?)
    }

    pub fn decision(&self, id: &str) -> Result<model::DecisionDetail, Error> {
        if self.backend == Backend::Nett {
            return self.nett_decision(id);
        }
        self.get(&["decisions", id], &[])
    }

    pub fn comments(&self, reference: &str, page: &PageRequest) -> Result<Page<Comment>, Error> {
        self.get(
            &["issues", reference, "comments"],
            &page.parameters(query::MAX_COMMENT_PAGE_SIZE)?,
        )
    }

    pub fn replies(
        &self,
        reference: &str,
        root: &str,
        page: &PageRequest,
    ) -> Result<Page<Comment>, Error> {
        self.get(
            &["issues", reference, "comments", root, "replies"],
            &page.parameters(query::MAX_COMMENT_PAGE_SIZE)?,
        )
    }

    pub fn relations(&self, reference: &str, page: &PageRequest) -> Result<Page<Relation>, Error> {
        self.get(
            &["issues", reference, "relations"],
            &page.parameters(query::MAX_RELATION_PAGE_SIZE)?,
        )
    }

    pub fn pull_requests(
        &self,
        reference: &str,
        page: &PageRequest,
    ) -> Result<Page<model::PullRequest>, Error> {
        if self.backend == Backend::Nett {
            return self.nett_pull_requests(reference, page);
        }
        self.get(
            &["issues", reference, "pull-requests"],
            &page.parameters(query::MAX_RELATION_PAGE_SIZE)?,
        )
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

impl reference::IssueResolver for Client {
    type Issue = IssueDetail;
    type Error = Error;
    fn resolve(&self, reference: &str) -> Result<IssueDetail, Error> {
        self.issue(reference)
    }
}

#[cfg(test)]
mod tests;
