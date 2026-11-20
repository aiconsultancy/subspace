mod identity;
use crate::{
    ListQuery,
    model::{IssueGroups, Me, Project},
    resolution_cache::{ResolutionCache, ResolutionScope},
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
pub(super) mod activity;
pub(super) mod issue;
pub(super) mod reference;
mod reference_errors;
use reference::ReferenceCard;
use reference_errors::ReferenceErrors;
pub(super) const STALE_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScopeGeneration(pub(super) u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubStatus {
    Loading,
    Ready,
    Error(ReadError),
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    SignInNeeded,
    Forbidden,
    Network,
    Response { status: Option<u16> },
}

impl From<crate::Error> for ReadError {
    fn from(error: crate::Error) -> Self {
        match error {
            crate::Error::SignInNeeded => Self::SignInNeeded,
            crate::Error::Forbidden => Self::Forbidden,
            crate::Error::Network => Self::Network,
            crate::Error::NotFound => Self::Response {
                status: Some(ureq::http::StatusCode::NOT_FOUND.as_u16()),
            },
            crate::Error::UnexpectedStatus(status) => Self::Response {
                status: Some(status),
            },
            _ => Self::Response { status: None },
        }
    }
}

#[derive(Clone)]
// qual:allow(srp, god_struct) reason: "One immutable versioned snapshot projects the connection and independently retained issue and list views."
pub struct HubSnapshot {
    pub(super) version: u64,
    pub(super) generation: ScopeGeneration,
    pub(super) profile_id: String,
    pub(super) profile_name: String,
    pub(super) me: Option<Me>,
    pub(super) identity: Option<crate::initiatives::IdentityObservation>,
    pub(super) projects: Vec<Project>,
    pub(super) selected_project: Option<String>,
    pub(super) status: HubStatus,
    pub(super) list: Option<IssueGroups>,
    pub(super) list_id: u64,
    pub(super) list_revision: u64,
    pub(super) issue_revision: u64,
    pub(super) list_loading: bool,
    pub(super) hidden_subissues_count: Option<u32>,
    pub(super) my_issues_count: Option<u32>,
    pub(super) updated_at: Option<Instant>,
    pub(super) page_loading: Vec<usize>,
    pub(super) page_errors: Vec<usize>,
    pub(super) issue: Option<issue::IssueSnapshot>,
    pub(super) activity: Option<activity::ActivitySnapshot>,
    pub(super) live: super::LiveStatus,
}

impl HubSnapshot {
    /// How changes made elsewhere reach this snapshot.
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn live(&self) -> super::LiveStatus {
        self.live
    }

    pub fn list_project(&self) -> Option<&str> {
        self.selected_project.as_deref()
    }

    pub fn identity(&self) -> Option<&crate::initiatives::IdentityObservation> {
        self.identity.as_ref()
    }

    pub fn me(&self) -> Option<&Me> {
        self.me.as_ref()
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn version(&self) -> u64 {
        self.version
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn generation(&self) -> ScopeGeneration {
        self.generation
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn status(&self) -> &HubStatus {
        &self.status
    }
    pub fn list(&self) -> Option<&IssueGroups> {
        self.list.as_ref()
    }
    pub fn list_id(&self) -> Option<u64> {
        self.list.as_ref().map(|_| self.list_id)
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn list_revision(&self) -> u64 {
        self.list_revision
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn issue_revision(&self) -> u64 {
        self.issue_revision
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn list_loading(&self) -> bool {
        self.list_loading
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn updated_at(&self) -> Option<Instant> {
        self.updated_at
    }
    pub fn page_loading(&self, group: usize) -> bool {
        self.page_loading.contains(&group)
    }
    pub fn page_failed(&self, group: usize) -> bool {
        self.page_errors.contains(&group)
    }
    pub fn age(&self, now: Instant) -> Option<Duration> {
        self.updated_at.map(|at| now.saturating_duration_since(at))
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn hidden_subissues_count(&self) -> Option<u32> {
        self.hidden_subissues_count
    }
    // qual:allow(dry, boilerplate) reason: "Readers preserve the immutable snapshot boundary; mutation belongs to the hub."
    pub fn my_issues_count(&self) -> Option<u32> {
        self.my_issues_count
    }
    pub fn issue(&self) -> Option<&issue::IssueSnapshot> {
        self.issue.as_ref()
    }
    pub fn activity(&self) -> Option<&activity::ActivitySnapshot> {
        self.activity.as_ref()
    }
}

pub(super) struct State {
    pub snapshot: HubSnapshot,
    pub initial_access_retry: bool,
    pub api_origin: String,
    pub query: Option<ListQuery>,
    pub revision: u64,
    pub issue_revision: u64,
    pub media_revision: u64,
    pub activity_revision: [u64; 2],
    pub pending_activity: Option<crate::ActivityQuery>,
    pub references: ResolutionCache<ReferenceCard>,
    pub reference_errors: ReferenceErrors,
    reference_count: usize,
    pub drafts: super::comments::Drafts,
    pub attachment_buffers: Vec<std::sync::Weak<[u8]>>,
    pub live_reads: super::live::LiveReads,
    pub project_refresh: super::live::ProjectRefresh,
}

impl State {
    pub fn new(origin: &str, name: &str) -> Self {
        Self {
            snapshot: HubSnapshot {
                version: 0,
                generation: ScopeGeneration::default(),
                profile_id: crate::PROFILE_ID.into(),
                profile_name: name.into(),
                me: None,
                identity: None,
                projects: vec![],
                selected_project: None,
                status: HubStatus::Loading,
                list: None,
                list_id: 0,
                list_revision: 0,
                issue_revision: 0,
                list_loading: false,
                hidden_subissues_count: None,
                my_issues_count: None,
                updated_at: None,
                page_loading: vec![],
                page_errors: vec![],
                issue: None,
                activity: None,
                live: super::LiveStatus::Connecting,
            },
            initial_access_retry: true,
            api_origin: origin.into(),
            query: None,
            revision: 0,
            issue_revision: 0,
            media_revision: 0,
            activity_revision: [0; 2],
            pending_activity: None,
            references: ResolutionCache::new(ResolutionScope::new(crate::PROFILE_ID, origin, 0)),
            reference_errors: ReferenceErrors::default(),
            reference_count: 0,
            drafts: HashMap::new(),
            attachment_buffers: Vec::new(),
            live_reads: super::live::LiveReads::default(),
            project_refresh: super::live::ProjectRefresh::default(),
        }
    }
    pub fn is_running(&self) -> bool {
        self.snapshot.status != HubStatus::Stopped
    }
    pub fn references_live(&self) -> bool {
        self.snapshot.me.is_some()
            && self.snapshot.status != HubStatus::Stopped
            && !matches!(
                self.snapshot.status,
                HubStatus::Error(ReadError::SignInNeeded | ReadError::Forbidden)
            )
    }
    pub fn running(&self) -> Result<(), super::HubError> {
        if self.is_running() {
            Ok(())
        } else {
            Err(super::HubError::Stopped)
        }
    }
    pub fn current(&self, generation: ScopeGeneration) -> bool {
        self.snapshot.generation == generation && self.is_running()
    }
    pub fn issue_current(&self, generation: ScopeGeneration, revision: u64) -> bool {
        self.current(generation) && self.issue_revision == revision
    }
    pub fn issue_key(&self) -> super::live::ReadKey {
        (self.snapshot.generation, self.issue_revision)
    }
    pub fn list_key(&self) -> super::live::ReadKey {
        (self.snapshot.generation, self.revision)
    }
    pub fn changed(&mut self) {
        self.snapshot.list_revision = self.revision;
        self.snapshot.issue_revision = self.issue_revision;
        self.snapshot.version = self.snapshot.version.wrapping_add(1);
    }
    pub fn install_list(&mut self, groups: IssueGroups) {
        self.snapshot.list_loading = false;
        self.snapshot.list = Some(groups);
        self.snapshot.list_id = self.snapshot.list_id.wrapping_add(1);
        self.snapshot.status = HubStatus::Ready;
        self.snapshot.updated_at = Some(Instant::now());
    }
    pub fn reset_references(&mut self) {
        self.references.set_scope(ResolutionScope::new(
            crate::PROFILE_ID,
            &self.api_origin,
            self.snapshot.generation.0,
        ));
        self.reference_errors.clear();
    }

    pub fn reference_version(&mut self, now: Instant) -> u64 {
        let count = self.references.cards(now).count();
        if count != self.reference_count {
            self.reference_count = count;
            self.changed();
        }
        self.snapshot.version
    }
}

impl std::fmt::Debug for HubSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubSnapshot")
            .field("version", &self.version)
            .field("generation", &self.generation)
            .field("profile_id", &self.profile_id)
            .field("status", &self.status)
            .field("project_count", &self.projects.len())
            .field("list", &self.list)
            .field("issue", &self.issue)
            .field("activity", &self.activity)
            .finish_non_exhaustive()
    }
}
