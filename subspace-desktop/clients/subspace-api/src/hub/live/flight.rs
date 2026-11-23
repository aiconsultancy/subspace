//! At most one live re-read in flight per target, and one trailing read when
//! hints arrived while it was out.
use crate::hub::ScopeGeneration;
use std::time::{Duration, Instant};

/// A read held longer than this is taken as lost, so hints are never stuck
/// behind it.
const STUCK: Duration = Duration::from_secs(60);

/// The scope a read belongs to: the hub's generation and the issue or list
/// revision. A read of an older scope never blocks the current one.
pub(in crate::hub) type ReadKey = (ScopeGeneration, u64);

#[derive(Debug, Default)]
pub(in crate::hub) struct Flight {
    busy: Option<(ReadKey, Instant)>,
    again: bool,
}

impl Flight {
    /// True when a read may start now; otherwise one trailing read is owed.
    pub(in crate::hub) fn begin(&mut self, key: ReadKey, now: Instant) -> bool {
        let busy = self.busy.is_some_and(|(held, since)| {
            held == key && now.saturating_duration_since(since) < STUCK
        });
        if busy {
            self.again = true;
            return false;
        }
        self.busy = Some((key, now));
        self.again = false;
        true
    }

    /// Whether a read is out, or one more is owed after it.
    pub(in crate::hub) fn pending(&self) -> bool {
        self.busy.is_some() || self.again
    }

    /// The read out no longer covers what is shown: one more is owed.
    pub(in crate::hub) fn owe(&mut self) {
        self.again = true;
    }

    /// The read for `key` finished; true when a trailing read is owed.
    pub(in crate::hub) fn land(&mut self, key: ReadKey) -> bool {
        if self.busy.is_none_or(|(held, _)| held != key) {
            return false;
        }
        self.busy = None;
        std::mem::take(&mut self.again)
    }
}

/// The live re-reads the hub may have out: the open issue and the list head.
#[derive(Debug, Default)]
pub(in crate::hub) struct LiveReads {
    pub issue: Flight,
    pub list: Flight,
    // A normal first load may still have an older response in flight.
    pub issue_after_load: Option<ReadKey>,
    pub list_after_load: Option<ReadKey>,
    /// Streams that turned live, and the catch-ups their `ready` began, so
    /// a test can wait for the catch-up that follows the latest `ready`.
    #[cfg(test)]
    pub readies: u64,
    #[cfg(test)]
    pub ready_catch_ups: u64,
}

#[cfg(test)]
mod tests {
    use super::{Flight, STUCK, ScopeGeneration};
    use std::time::{Duration, Instant};

    #[test]
    fn one_read_in_flight_and_one_trailing_read() {
        let now = Instant::now();
        let key = (ScopeGeneration::default(), 1);
        let mut flight = Flight::default();
        assert!(flight.begin(key, now));
        assert!(!flight.begin(key, now));
        assert!(!flight.begin(key, now));
        assert!(flight.land(key));
        assert!(flight.begin(key, now));
        assert!(!flight.land(key));
    }

    #[test]
    fn other_scopes_and_lost_reads_never_block() {
        let now = Instant::now();
        let (old, new) = (
            (ScopeGeneration::default(), 1),
            (ScopeGeneration::default(), 2),
        );
        let mut flight = Flight::default();
        assert!(flight.begin(old, now));
        assert!(flight.begin(new, now));
        assert!(
            !flight.land(old),
            "an older scope's reply leaves the new read out"
        );
        assert!(!flight.begin(new, now));
        assert!(flight.begin(new, now + STUCK + Duration::from_millis(1)));
    }
}
