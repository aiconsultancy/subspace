//! Reading the memberships again, with backoff, while an assigned fleet
//! could not be read.

use std::time::Duration;

use gpui_kit::Task;

/// The first wait before reading memberships again after a fleet lookup
/// failed; each further failure doubles it, up to the maximum.
const FLEET_RETRY_FIRST: Duration = Duration::from_secs(5);
pub(super) const FLEET_RETRY_MAX: Duration = Duration::from_secs(300);
const FLEET_RETRY_MAX_DOUBLINGS: u32 = 6;

/// Reads of the memberships again while an assigned fleet could not be
/// read, with backoff.
#[derive(Default)]
pub(super) struct FleetRetry {
    pub(super) task: Option<Task<()>>,
    pub(super) failures: u32,
}

impl FleetRetry {
    /// The wait before the next retry, counting this failure, or `None`
    /// once no assigned fleet is `unresolved`.
    pub(super) fn next(&mut self, unresolved: bool) -> Option<Duration> {
        if !unresolved {
            self.failures = 0;
            return None;
        }
        let doublings = self.failures.min(FLEET_RETRY_MAX_DOUBLINGS);
        self.failures = self.failures.saturating_add(1);
        Some(
            FLEET_RETRY_FIRST
                .saturating_mul(1 << doublings)
                .min(FLEET_RETRY_MAX),
        )
    }
}
