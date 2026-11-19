//! Live previews/viewers keep revalidation active; unused Channels sessions do no API polling.
use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
};
const ACTIVE_FOR: Duration = Duration::from_secs(60);
#[derive(Default)]
pub(crate) struct Demand {
    active: Option<Instant>,
    holds: Vec<Weak<()>>,
}
impl Demand {
    pub(crate) fn activate(&mut self) {
        self.active = Some(Instant::now());
    }
    pub(crate) fn hold(&mut self) -> Arc<()> {
        self.activate();
        let lease = Arc::new(());
        self.holds.push(Arc::downgrade(&lease));
        lease
    }
    pub(super) fn active_now(&mut self) -> bool {
        self.holds.retain(|lease| lease.strong_count() > 0);
        !self.holds.is_empty() || self.active.is_some_and(|at| at.elapsed() < ACTIVE_FOR)
    }
}
#[cfg(test)]
mod tests {
    use super::Demand;
    #[test]
    fn open_uploads_keep_identity_checks_alive_until_the_preview_or_viewer_closes() {
        let mut session = Demand::default();
        let lease = session.hold();
        session.active = Some(std::time::Instant::now() - std::time::Duration::from_secs(120));
        assert!(session.active_now());
        drop(lease);
        assert!(!session.active_now());
    }
}
