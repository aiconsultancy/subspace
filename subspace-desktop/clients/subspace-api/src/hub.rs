//! Shared memory snapshots and bounded background readers, with generation fences.
mod activity;
mod comments;
mod files;
mod images;
mod issue;
mod issue_writes;
mod live;
pub use live::LiveStatus;
mod pull_requests;
pub use files::FilesSnapshot;
pub use images::ImagesSnapshot;
pub use issue_writes::IssueWriteScope;
mod queue;
mod reference;
mod rescan;
mod search;
mod state;
mod timeline;
mod worker;
use crate::{Client, ListQuery};
pub use comments::{Attachment, AttachmentState, CommentDraft, CommentState, ImageIntake};
pub use queue::ReadPriority;
pub use search::WiderSearch;
use state::State;
pub use state::activity::{ActivityListSnapshot, ActivitySection, ActivitySnapshot};
pub use state::issue::{IssueSnapshot, IssueStatus, TimelineSnapshot};
pub use state::reference::{IrcSnapshot, ReferenceCard, ReferenceFacts, ResolvedReference};
pub use state::{HubSnapshot, HubStatus, ReadError, ScopeGeneration};
use std::{
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::Instant,
};
use worker::{Job, Queue, Task};

static CURRENT: Mutex<Option<Weak<Shared>>> = Mutex::new(None);

fn shared_running(shared: &Arc<Shared>) -> bool {
    lock(&shared.state).is_running()
}

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("This issue profile is not available.")]
    InvalidProfile,
    #[error("The Issues hub has stopped.")]
    Stopped,
    #[error("Search must be at most 200 characters.")]
    SearchTooLong,
    #[error("The Issues list filters are invalid or contradictory.")]
    InvalidQuery,
    #[error(transparent)]
    Client(#[from] crate::Error),
}

#[derive(Clone)]
pub struct Hub(Arc<Shared>);
struct Shared {
    client: Client,
    state: Mutex<State>,
    queue: Arc<Queue>,
    issue_writes: Mutex<issue_writes::Writes>,
    file_slots: Arc<tokio::sync::Semaphore>,
    upload_lock: Mutex<()>,
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    fn enqueue(&self, state: &mut State, task: Task, priority: ReadPriority) {
        self.queue.push(Job::new(state, task), priority);
    }
}

impl Hub {
    pub fn start() -> Result<Self, HubError> {
        let mut current = lock(&CURRENT);
        match current
            .as_ref()
            .and_then(Weak::upgrade)
            .filter(shared_running)
        {
            Some(shared) => Ok(Self(shared)),
            None => Self::register(&mut current),
        }
    }

    fn register(current: &mut Option<Weak<Shared>>) -> Result<Self, HubError> {
        let hub = Self::from_client(Client::new()?, crate::PROFILE_NAME);
        *current = Some(Arc::downgrade(&hub.0));
        Ok(hub)
    }

    pub fn from_client(client: Client, name: &str) -> Self {
        let state = State::new(client.api_origin(), name);
        let shared = Arc::new(Shared {
            client,
            state: Mutex::new(state),
            queue: Arc::new(Queue::default()),
            issue_writes: Mutex::new(Default::default()),
            file_slots: Arc::new(tokio::sync::Semaphore::new(files::QUEUE_CAPACITY)),
            upload_lock: Mutex::new(()),
        });
        worker::start(&shared);
        live::start(&shared);
        let hub = Self(shared);
        let mut state = lock(&hub.0.state);
        hub.0
            .enqueue(&mut state, Task::Bootstrap, ReadPriority::Visible);
        drop(state);
        hub
    }

    pub fn snapshot(&self) -> HubSnapshot {
        lock(&self.0.state).snapshot.clone()
    }
    /// A hub's data and queued requests belong to its original account.
    pub fn account_changed(&self) -> bool {
        self.0.client.account_changed()
    }
    pub fn version(&self) -> u64 {
        lock(&self.0.state).snapshot.version
    }
    pub fn activate(&self, query: ListQuery) -> Result<(), HubError> {
        self.request_list(query)
    }
    pub fn reload(&self, query: ListQuery) -> Result<(), HubError> {
        query
            .grouped_parameters()
            .map_err(|_| HubError::InvalidQuery)?;
        let mut state = lock(&self.0.state);
        state.running()?;
        state.select_project(query.selected_project());
        state.query = Some(query);
        state.revision = state.revision.wrapping_add(1);
        state.snapshot.list_loading = true;
        state.snapshot.page_loading.clear();
        state.snapshot.page_errors.clear();
        state.changed();
        self.0
            .enqueue(&mut state, Task::Bootstrap, ReadPriority::Visible);
        Ok(())
    }

    /// Refresh identity and projects without disturbing another view's list.
    pub fn reload_projects(&self) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        self.0
            .enqueue(&mut state, Task::ReloadProjects, ReadPriority::Visible);
        Ok(())
    }

    /// Release the visible list and Activity queries while the initiative has no projects.
    pub fn clear_project_scope(&self) {
        let mut state = lock(&self.0.state);
        if !state.is_running() || !state.has_project_scope() {
            return;
        }
        state.snapshot.selected_project = None;
        state.query = None;
        state.clear_list();
        state.clear_activity();
        state.changed();
    }

    fn request_list(&self, query: ListQuery) -> Result<(), HubError> {
        if query.search_too_long() {
            return Err(HubError::SearchTooLong);
        }
        query
            .grouped_parameters()
            .map_err(|_| HubError::InvalidQuery)?;
        let mut state = lock(&self.0.state);
        state.running()?;
        state.select_project(query.selected_project());
        let changed = state.query.as_ref() != Some(&query);
        let needed = changed
            || state
                .snapshot
                .age(Instant::now())
                .is_none_or(|age| age > state::STALE_AFTER);
        state.query = Some(query.clone());
        if !needed || state.snapshot.me.is_none() {
            return Ok(());
        }
        if changed {
            state.snapshot.list = None;
            state.snapshot.updated_at = None;
        }
        state.revision = state.revision.wrapping_add(1);
        state.snapshot.list_loading = true;
        state.snapshot.page_errors.clear();
        state.snapshot.page_loading.clear();
        state.changed();
        self.0
            .enqueue(&mut state, Task::List(query), ReadPriority::Visible);
        Ok(())
    }

    pub fn load_more(&self, group: usize) -> Result<(), HubError> {
        let mut state = lock(&self.0.state);
        state.running()?;
        if state.snapshot.list_loading || state.snapshot.page_loading.contains(&group) {
            return Ok(());
        }
        let Some(mut selected) = state
            .snapshot
            .list
            .as_ref()
            .and_then(|list| list.groups.get(group))
            .cloned()
        else {
            return Ok(());
        };
        if selected.items.len() >= selected.count as usize
            || (selected.next_cursor.is_none() && !selected.items.is_empty())
        {
            return Ok(());
        }
        let Some(query) = state.query.clone() else {
            return Ok(());
        };
        selected.items.clear();
        state.snapshot.page_loading.push(group);
        state.snapshot.page_errors.retain(|index| *index != group);
        state.changed();
        let list = state.snapshot.list_id;
        self.0.enqueue(
            &mut state,
            Task::Page {
                query,
                group,
                selected,
                list,
            },
            ReadPriority::LaterPage,
        );
        Ok(())
    }

    pub fn stop(&self) {
        self.retire_comment_uploads();
        let mut state = lock(&self.0.state);
        state.snapshot.status = HubStatus::Stopped;
        state.snapshot.generation.0 = state.snapshot.generation.0.wrapping_add(1);
        state.drafts.clear();
        state.snapshot.issue = None;
        state.snapshot.activity = None;
        state.snapshot.list = None;
        state.snapshot.me = None;
        state.snapshot.projects.clear();
        state.reset_references();
        state.changed();
        drop(state);
        *lock(&self.0.issue_writes) = Default::default();
        self.0.file_slots.close();
        self.0.queue.stop();
    }

    /// Reads the list head again as a live hint would.
    #[cfg(feature = "test-support")]
    // qual:test_helper
    pub fn reread_list_for_test(&self) {
        let mut state = lock(&self.0.state);
        live::reread_list(&self.0, &mut state);
    }

    #[cfg(feature = "test-support")]
    // qual:test_helper
    pub fn set_list_updated_at_for_test(&self, at: Option<Instant>) {
        let mut state = lock(&self.0.state);
        state.snapshot.updated_at = at;
        state.changed();
    }
}
