use super::{ReadPriority, ScopeGeneration, Shared, State, lock, queue::PriorityQueue};
use crate::{ListQuery, model::IssueGroup};
use std::sync::{Arc, Condvar, Mutex, Weak};
mod connection;
pub(super) mod detail;
mod list;
pub(super) mod pages;
mod timeline;

pub(super) enum Task {
    Bootstrap,
    RefreshProjects(u64),
    ReloadProjects,
    List(ListQuery),
    SearchProbe(super::search::Probe),
    ListCounts(ListQuery),
    /// A group's next page for list installation `list`.
    Page {
        query: ListQuery,
        group: usize,
        selected: IssueGroup,
        list: u64,
    },
    Activity {
        query: crate::ActivityQuery,
        section: super::ActivitySection,
        cursor: Option<String>,
    },
    Issue(String),
    Milestone(String, String),
    Images(String, Option<String>),
    Files(String, Option<String>),
    MediaBytes(super::files::FileRead),
    /// Sub-issues past the first page, for that collection's epoch.
    Children(crate::IssueQuery, u64),
    /// Relations past the first page, for that collection's epoch.
    Relations(String, crate::PageRequest, u64),
    /// Pull requests; a continuation applies only to its epoch.
    PullRequests(String, crate::PageRequest, u64),
    Decisions(super::decisions::DecisionsRequest),
    Decision(super::decision::DecisionRequest),
    Timeline(super::timeline::TimelineRequest),
    Refresh(String),
    /// A live hint: the open issue again, keeping what is shown.
    Recheck(String),
    /// A live hint: the list's first pages again, keeping paged rows.
    ListHead(ListQuery),
    Reference(crate::resolution_cache::ResolutionRead),
    Discussion(super::comments::DraftKey),
}

pub(super) struct Job {
    pub generation: ScopeGeneration,
    pub revision: u64,
    pub issue_revision: u64,
    timeline_epoch: u64,
    pub media_revision: u64,
    pub activity_revision: u64,
    pub task: Task,
}

impl Job {
    pub fn new(state: &State, task: Task) -> Self {
        let activity_revision = if let Task::Activity { section, .. } = &task {
            state.activity_revision[*section as usize]
        } else {
            0
        };
        Self {
            generation: state.snapshot.generation,
            revision: state.revision,
            issue_revision: state.issue_revision,
            timeline_epoch: state
                .snapshot
                .issue
                .as_ref()
                .map_or(0, |issue| issue.timeline.epoch),
            media_revision: state.media_revision,
            activity_revision,
            task,
        }
    }
    pub fn current(&self, state: &State) -> bool {
        if !state.current(self.generation) {
            return false;
        }
        if matches!(self.task, Task::Timeline(_) | Task::Refresh(_))
            && !state
                .snapshot
                .issue
                .as_ref()
                .is_some_and(|issue| issue.timeline.epoch == self.timeline_epoch)
        {
            return false;
        }
        match &self.task {
            Task::Decisions(request) => {
                state.issue_current(self.generation, self.issue_revision)
                    && super::decisions::current(state, request)
            }
            Task::Decision(request) => {
                state.issue_current(self.generation, self.issue_revision)
                    && super::decision::current(state, request)
            }
            Task::List(_)
            | Task::ListHead(_)
            | Task::SearchProbe(_)
            | Task::ListCounts(_)
            | Task::Page { .. } => self.revision == state.revision,
            Task::Activity { section, .. } => {
                state.activity_current(self.generation, self.activity_revision, *section)
            }
            Task::Images(..) | Task::Files(..) => {
                state.issue_current(self.generation, self.issue_revision)
                    && state.media_revision == self.media_revision
            }
            Task::Issue(_)
            | Task::Milestone(..)
            | Task::MediaBytes(_)
            | Task::Children(..)
            | Task::Relations(..)
            | Task::PullRequests(..)
            | Task::Timeline(_)
            | Task::Refresh(_)
            | Task::Recheck(_) => state.issue_current(self.generation, self.issue_revision),
            _ => true,
        }
    }
}

#[derive(Default)]
pub(super) struct Queue {
    pending: Mutex<Pending>,
    ready: Condvar,
}
#[derive(Default)]
struct Pending {
    jobs: PriorityQueue<Job>,
    stopped: bool,
    reference_active: bool,
}
impl Queue {
    pub fn push(&self, job: Job, priority: ReadPriority) {
        let mut pending = lock(&self.pending);
        if !pending.stopped {
            pending.jobs.push(priority, job);
            self.ready.notify_one();
        }
    }
    fn next(&self) -> Option<Job> {
        let mut pending = lock(&self.pending);
        loop {
            if pending.stopped {
                return None;
            }
            let references = !pending.reference_active;
            if let Some(job) = pending.jobs.pop_for(references) {
                pending.reference_active |= matches!(job.task, Task::Reference(_));
                return Some(job);
            }
            pending = self
                .ready
                .wait(pending)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
    pub fn stop(&self) {
        let mut pending = lock(&self.pending);
        pending.stopped = true;
        pending.jobs = PriorityQueue::default();
        self.ready.notify_all();
    }
}

pub(super) fn start(shared: &Arc<Shared>) {
    for _ in 0..2 {
        let weak = Arc::downgrade(shared);
        let queue = shared.queue.clone();
        std::thread::spawn(move || reader(weak, queue));
    }
}

fn reader(weak: Weak<Shared>, queue: Arc<Queue>) {
    while let Some(job) = queue.next() {
        let _lease = ReaderLease {
            queue: &queue,
            reference: matches!(job.task, Task::Reference(_)),
        };
        let Some(shared) = weak.upgrade() else {
            return;
        };
        if !job.current(&lock(&shared.state)) {
            continue;
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dispatch(&shared, job)));
    }
}

struct ReaderLease<'a> {
    queue: &'a Queue,
    reference: bool,
}
impl Drop for ReaderLease<'_> {
    fn drop(&mut self) {
        if self.reference {
            lock(&self.queue.pending).reference_active = false;
            self.queue.ready.notify_all();
        }
    }
}

fn dispatch(shared: &Shared, job: Job) {
    match &job.task {
        Task::Bootstrap | Task::ReloadProjects => connection::bootstrap(shared, &job),
        Task::RefreshProjects(revision) => connection::refresh_projects(shared, &job, *revision),
        Task::List(query) => list::load(shared, &job, query),
        Task::SearchProbe(probe) => super::search::probe(shared, &job, probe),
        Task::ListCounts(query) => list::counts(shared, &job, query),
        Task::Page {
            query,
            group,
            selected,
            list,
        } => list::page(shared, &job, (query, *group, selected, *list)),
        Task::Activity {
            query,
            section,
            cursor,
        } => super::activity::load(shared, &job, (query, *section, cursor.as_deref())),
        Task::Images(issue, cursor) => super::images::load(shared, &job, issue, cursor.as_deref()),
        Task::Files(issue, cursor) => super::files::load(shared, &job, issue, cursor.as_deref()),
        Task::MediaBytes(request) => super::files::bytes(shared, &job, request),
        Task::Issue(identifier) => detail::load(shared, &job, identifier),
        Task::Milestone(project, id) => detail::milestone::load(shared, &job, project, id),
        Task::Children(query, epoch) => detail::children(shared, &job, query, *epoch),
        Task::Relations(identifier, page, epoch) => {
            detail::relations(shared, &job, (identifier, page, *epoch))
        }
        Task::PullRequests(identifier, page, epoch) => {
            super::pull_requests::load(shared, &job, (identifier, page, *epoch))
        }
        Task::Timeline(request) => timeline::load(shared, &job, request.clone()),
        Task::Decisions(request) => super::decisions::load(shared, &job, request),
        Task::Decision(request) => super::decision::load(shared, &job, request),
        Task::Refresh(identifier) => timeline::refresh(shared, &job, identifier),
        Task::Recheck(identifier) => detail::recheck(shared, &job, identifier),
        Task::ListHead(query) => list::head(shared, &job, query),
        Task::Discussion(key) => super::comments::check(shared, &job, key),
        Task::Reference(_) => super::reference::resolve(shared, job),
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.queue.stop();
    }
}
