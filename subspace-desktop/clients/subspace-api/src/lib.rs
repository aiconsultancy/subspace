//! Blocking Subspace API reads and writes. Use the hub or a background task.
mod activity;
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
mod pacing;
mod query;
pub mod reference;
pub mod resolution_cache;
mod transport;
pub mod uploads;
mod writing;
pub use activity::ActivityQuery;
pub use dev_sign_in::{DevSignIn, DevUser};
pub use fixture::fixture_from_env;
pub use list::{ListQuery, ListTab};
use model::{
    Comment, IssueDetail, IssueGroups, IssueListItem, Me, Page, Project, Relation, WorkflowState,
};
pub use native::{api_base, configure_api_origin, configured_api_origin, use_native_account};
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
    /// Whether a hub built on this client opens the issue update stream.
    live: bool,
    native_lease: Option<NativeLease>,
}

impl Client {
    pub fn new() -> Result<Self, Error> {
        // Debug fixtures (capture, offline servers) pair a loopback URL with
        // a private token file. Without the file, the configured origin and
        // the signed-in account apply, as in release builds.
        #[cfg(debug_assertions)]
        {
            let path = std::env::var_os("SUBSPACE_API_ACCESS_JWT_FILE");
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
        Ok(Self {
            base: native::origin().ok_or(Error::InvalidRequest)?.clone(),
            access_jwt: None,
            live: true,
            agent: native::agent(native_generation),
            native_lease: Some(NativeLease(native_generation)),
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
        })
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
    /// Whether this client belongs to a superseded native account.
    pub fn account_changed(&self) -> bool {
        self.native_lease
            .is_some_and(|NativeLease(generation)| generation != native::generation())
    }
    pub fn me(&self) -> Result<Me, Error> {
        self.get(&["me"], &[])
    }

    /// Initiative memberships of the current Subspace API caller. An item
    /// that names only its fleet's ID gets the fleet read separately. If that
    /// read fails, the item keeps its `fleet_id` with no `fleet`: callers must
    /// treat it as unresolved, never as the default server, and the shared
    /// initiative catalogue reads it again with backoff.
    pub fn member_initiatives(&self) -> Result<Vec<model::MemberInitiative>, Error> {
        let page: Page<model::MemberInitiative> = self.get(&["me", "initiatives"], &[])?;
        let mut items = page.items;
        for item in items.iter_mut().filter(|item| item.fleet.is_none()) {
            if let Some(id) = item.fleet_id.clone() {
                item.fleet = self.fleet(&id).ok();
            }
        }
        Ok(items)
    }

    /// One fleet by ID or slug.
    pub fn fleet(&self, id: &str) -> Result<model::Fleet, Error> {
        self.get(&["fleets", id], &[])
    }

    pub fn projects(&self, page: &PageRequest) -> Result<Page<Project>, Error> {
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

    // qual:api — read client for the decision UI; contract-tested here.
    pub fn decisions(
        &self,
        query: &DecisionQuery,
        page: &PageRequest,
    ) -> Result<Page<model::DecisionListItem>, Error> {
        self.get(&["decisions"], &query.parameters(page)?)
    }

    // qual:api — read client for the decision UI; contract-tested here.
    pub fn decision(&self, id: &str) -> Result<model::DecisionDetail, Error> {
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
        let url = transport::endpoint(&self.base, path, query)?;
        transport::get(&self.agent, &url, self.access_jwt.as_ref())
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
