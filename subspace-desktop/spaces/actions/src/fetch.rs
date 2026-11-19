//! The board's one background fetch. Discovery and a full load cover every
//! run, so a run refresh never replaces them. A write accepted during a full
//! load queues its run for a refresh once that load lands, because the load
//! may have read the run before GitHub accepted the write. A full load
//! delivers one repository at a time and remembers which are still to come.

use std::collections::BTreeSet;

use gpui_kit::Task;

use crate::model::RunTarget;

/// Each running variant owns its task; dropping it cancels the fetch.
#[derive(Default)]
pub(crate) enum Fetch {
    #[default]
    Idle,
    Discovering {
        _task: Task<()>,
    },
    Loading {
        _task: Task<()>,
        follow_up: Vec<RunTarget>,
        /// Repositories whose records this load has not delivered yet.
        pending: BTreeSet<String>,
    },
    Refreshing {
        _task: Task<()>,
    },
}

impl Fetch {
    pub(crate) fn discovering(task: Task<()>) -> Self {
        Self::Discovering { _task: task }
    }

    pub(crate) fn loading(task: Task<()>, repos: &[String]) -> Self {
        Self::Loading {
            _task: task,
            follow_up: Vec::new(),
            pending: repos.iter().cloned().collect(),
        }
    }

    pub(crate) fn refreshing(task: Task<()>) -> Self {
        Self::Refreshing { _task: task }
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Whether discovery or a full load runs, which the status line says
    /// until the load ends. A run refresh is not one.
    pub(crate) fn loads_records(&self) -> bool {
        matches!(self, Self::Discovering { .. } | Self::Loading { .. })
    }

    pub(crate) fn loads_repo(&self, repo: &str) -> bool {
        matches!(self, Self::Loading { pending, .. } if pending.contains(repo))
    }

    /// The repositories of `repos` whose records are still on their way:
    /// all of them while accounts are discovered, and those a full load has
    /// not delivered yet. A run refresh loads none: its runs stay shown.
    pub(crate) fn loading_repos(&self, repos: &[String]) -> BTreeSet<String> {
        match self {
            Self::Discovering { .. } => repos.iter().cloned().collect(),
            Self::Loading { pending, .. } => pending.clone(),
            Self::Idle | Self::Refreshing { .. } => BTreeSet::new(),
        }
    }

    /// A full load delivered `repo`'s records.
    // qual:allow(srp, nms) reason: "The pending set shrinks through the matched &mut pending, which the heuristic does not follow."
    pub(crate) fn delivered(&mut self, repo: &str) {
        if let Self::Loading { pending, .. } = self {
            pending.remove(repo);
        }
    }

    /// Ends the fetch. Returns the runs queued behind a full load.
    pub(crate) fn finish(&mut self) -> Vec<RunTarget> {
        let follow_up = match self {
            Self::Loading { follow_up, .. } => std::mem::take(follow_up),
            _ => Vec::new(),
        };
        *self = Self::Idle;
        follow_up
    }

    /// The run to refresh now after a write, or `None` when a running
    /// discovery or full load covers it.
    // qual:allow(srp, nms) reason: "The queue grows through the matched &mut follow_up, which the heuristic does not follow."
    pub(crate) fn after_write(&mut self, target: RunTarget) -> Option<RunTarget> {
        match self {
            Self::Discovering { .. } => None,
            Self::Loading { follow_up, .. } => {
                follow_up.push(target);
                None
            }
            Self::Idle | Self::Refreshing { .. } => Some(target),
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::Task;

    use super::Fetch;

    fn repos() -> Vec<String> {
        vec!["acme/web".into(), "acme/api".into()]
    }

    #[test]
    fn a_full_load_names_each_repository_until_it_is_delivered() {
        let repos = repos();
        assert!(Fetch::Idle.loading_repos(&repos).is_empty());
        assert_eq!(
            Fetch::discovering(Task::ready(()))
                .loading_repos(&repos)
                .len(),
            2
        );
        assert!(
            Fetch::refreshing(Task::ready(()))
                .loading_repos(&repos)
                .is_empty()
        );
        let mut fetch = Fetch::loading(Task::ready(()), &repos);
        fetch.delivered("acme/web");
        assert_eq!(
            fetch.loading_repos(&repos).into_iter().collect::<Vec<_>>(),
            ["acme/api"]
        );
        fetch.delivered("acme/api");
        assert!(fetch.loading_repos(&repos).is_empty());
        assert!(fetch.loads_records(), "the load ends only when it finishes");
        fetch.finish();
        assert!(fetch.is_idle());
        assert!(!fetch.loads_records());
        assert!(!Fetch::refreshing(Task::ready(())).loads_records());
    }
}
