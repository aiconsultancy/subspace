//! Retry timings and backoff.

use std::time::Duration;

/// Each retry waits between this share of its delay and all of it.
pub(super) const MIN_JITTER: f64 = 0.5;
/// The delay stops doubling after this many failures; `max_retry` caps it
/// well before.
const MAX_DOUBLINGS: u32 = 10;
/// Timings, shortened by tests.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pace {
    pub(crate) first_retry: Duration,
    pub(crate) max_retry: Duration,
    /// A stream up this long resets the backoff.
    pub(crate) stable: Duration,
    pub(crate) poll: Duration,
    /// How long to poll before trying the stream again.
    pub(crate) stream_retry: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            first_retry: Duration::from_secs(1),
            max_retry: Duration::from_secs(60),
            stable: Duration::from_secs(20),
            poll: Duration::from_secs(15),
            stream_retry: Duration::from_secs(120),
        }
    }
}

/// Exponential delays with jitter, from `first_retry` up to `max_retry`.
pub(super) struct Backoff {
    pace: Pace,
    failures: u32,
}

impl Backoff {
    pub(super) fn new(pace: Pace) -> Self {
        Self { pace, failures: 0 }
    }

    pub(super) fn reset(&mut self) {
        self.failures = 0;
    }

    /// The next delay, scaled by `jitter` (0.5 to 1).
    pub(super) fn next(&mut self, jitter: f64) -> Duration {
        let doubled = self
            .pace
            .first_retry
            .saturating_mul(1 << self.failures.min(MAX_DOUBLINGS));
        self.failures = self.failures.saturating_add(1);
        doubled.min(self.pace.max_retry).mul_f64(jitter)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Backoff, Pace};

    #[test]
    fn backoff_doubles_to_its_cap_and_resets() {
        let mut backoff = Backoff::new(Pace::default());
        let delays: Vec<_> = (0..8).map(|_| backoff.next(1.0)).collect();
        assert_eq!(delays[0], Duration::from_secs(1));
        assert_eq!(delays[3], Duration::from_secs(8));
        assert_eq!(delays[7], Duration::from_secs(60));
        assert_eq!(backoff.next(0.5), Duration::from_secs(30));
        backoff.reset();
        assert_eq!(backoff.next(1.0), Duration::from_secs(1));
    }
}
