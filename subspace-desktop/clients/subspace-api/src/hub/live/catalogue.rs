//! A missing stream can mean a subscribed project was archived elsewhere.
use super::{ReadPriority, Shared, Task, lock};
use std::{
    sync::Weak,
    time::{Duration, Instant},
};

const COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Default)]
pub(in crate::hub) struct ProjectRefresh {
    revision: u64,
    pending: bool,
    last: Option<Instant>,
}

impl ProjectRefresh {
    fn begin(&mut self, now: Instant) -> Option<u64> {
        if self.pending
            || self
                .last
                .is_some_and(|last| now.saturating_duration_since(last) < COOLDOWN)
        {
            return None;
        }
        self.pending = true;
        Some(self.revision)
    }

    pub(in crate::hub) fn finish(&mut self, revision: u64, now: Instant) -> bool {
        self.pending = false;
        self.last = Some(now);
        self.revision == revision
    }

    pub(in crate::hub) fn replaced(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

pub(super) fn request(weak: &Weak<Shared>, generation: crate::hub::ScopeGeneration) {
    let Some(shared) = weak.upgrade() else {
        return;
    };
    let mut state = lock(&shared.state);
    if state.current(generation)
        && state.snapshot.me.is_some()
        && let Some(revision) = state.project_refresh.begin(Instant::now())
    {
        shared.enqueue(
            &mut state,
            Task::RefreshProjects(revision),
            ReadPriority::LaterPage,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{COOLDOWN, ProjectRefresh};
    use std::time::{Duration, Instant};

    #[test]
    fn catalogue_recovery_has_one_read_at_a_time_and_cools_down_even_after_a_stale_read() {
        let mut refresh = ProjectRefresh::default();
        let start = Instant::now();
        let revision = refresh.begin(start).unwrap();
        assert_eq!(refresh.begin(start + COOLDOWN), None);
        refresh.replaced();
        let done = start + COOLDOWN;
        assert!(!refresh.finish(revision, done));
        assert_eq!(
            refresh.begin(done + COOLDOWN - Duration::from_millis(1)),
            None
        );
        assert_ne!(refresh.begin(done + COOLDOWN), Some(revision));
    }
}
