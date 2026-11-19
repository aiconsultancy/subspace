//! Timing for live updates: reconnect backoff and coalesced refreshes. Pure,
//! so tests can drive it with their own clock.
use super::flight::ReadKey;
use std::time::{Duration, Instant};

pub(crate) const MIN_BACKOFF: Duration = Duration::from_secs(1);
pub(crate) const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Hints arriving together are applied together.
pub(crate) const COALESCE: Duration = Duration::from_millis(300);
/// The open issue reads at most this often from hints; one recheck reads
/// its detail, activity, pull request and media heads.
pub(crate) const ISSUE_SPACING: Duration = if cfg!(test) {
    Duration::from_millis(600)
} else {
    Duration::from_secs(2)
};
/// The list head reads at most this often from hints.
pub(crate) const LIST_SPACING: Duration = if cfg!(test) {
    Duration::from_millis(900)
} else {
    Duration::from_secs(3)
};
/// Without the stream, the open issue reads again this often while shown.
pub(crate) const POLL: Duration = if cfg!(test) {
    Duration::from_millis(300)
} else {
    Duration::from_secs(30)
};
/// A connection live this long resets the backoff; one that drops sooner
/// keeps backing off, so a flapping stream is not reopened every second.
pub(crate) const STABLE: Duration = if cfg!(test) {
    Duration::from_secs(2)
} else {
    Duration::from_secs(60)
};
/// A stream must confirm it is ready within this time.
pub(crate) const READY_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_secs(3)
} else {
    Duration::from_secs(15)
};

/// Doublings before the delay reaches `MAX_BACKOFF` (1 s × 2⁵ > 30 s).
const DOUBLINGS: u32 = 5;
/// At most this share of the delay varies at random.
const JITTER: f64 = 0.25;

/// Exponential backoff from 1 s to 30 s, with bounded jitter on every attempt.
#[derive(Debug, Default)]
pub(crate) struct Backoff {
    failures: u32,
}

impl Backoff {
    pub(crate) fn reset(&mut self) {
        self.failures = 0;
    }

    /// The next delay; `fraction` in `0.0..=1.0` picks the jitter.
    pub(crate) fn next(&mut self, fraction: f64) -> Duration {
        let ceiling = MIN_BACKOFF
            .saturating_mul(1 << self.failures.min(DOUBLINGS))
            .min(MAX_BACKOFF);
        self.failures = self.failures.saturating_add(1);
        let jitter = fraction.clamp(0.0, 1.0) * JITTER;
        // Add the first jitter above the minimum. Later delays have room below.
        let factor = if ceiling == MIN_BACKOFF {
            1.0 + jitter
        } else {
            1.0 - jitter
        };
        ceiling.mul_f64(factor)
    }
}

/// One refresh target: waits `COALESCE` after its first hint and at least
/// `spacing` after its last read.
#[derive(Debug)]
pub(crate) struct Due {
    spacing: Duration,
    first: Option<(ReadKey, Instant)>,
    last: Option<Instant>,
}

impl Due {
    pub(crate) fn new(spacing: Duration) -> Self {
        Self {
            spacing,
            first: None,
            last: None,
        }
    }

    pub(crate) fn note(&mut self, key: ReadKey, now: Instant) {
        if self.first.is_none_or(|(held, _)| held != key) {
            self.first = Some((key, now));
        }
    }

    pub(crate) fn take(&mut self, key: ReadKey, now: Instant) -> bool {
        let Some((held, first)) = self.first else {
            return false;
        };
        if held != key {
            self.first = None;
            return false;
        }
        let waited = now.saturating_duration_since(first) >= COALESCE;
        let spaced = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.spacing);
        if waited && spaced {
            self.first = None;
            self.last = Some(now);
        }
        waited && spaced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_thirty_seconds_with_bounded_jitter() {
        let mut backoff = Backoff::default();
        let delays: Vec<_> = (0..7).map(|_| backoff.next(0.)).collect();
        assert_eq!(
            delays.iter().map(Duration::as_secs).collect::<Vec<_>>(),
            [1, 2, 4, 8, 16, 30, 30]
        );
        backoff.reset();
        assert_eq!(backoff.next(1.), Duration::from_millis(1250));
        assert_eq!(backoff.next(1.), Duration::from_millis(1500));
    }

    #[test]
    fn due_coalesces_then_spaces_reads() {
        let start = Instant::now();
        let mut due = Due::new(ISSUE_SPACING);
        let key = (crate::hub::ScopeGeneration::default(), 1);
        assert!(!due.take(key, start));
        due.note(key, start);
        due.note(key, start + Duration::from_millis(200));
        assert!(!due.take(key, start + Duration::from_millis(299)));
        assert!(due.take(key, start + COALESCE));
        assert!(!due.take(key, start + COALESCE));
        due.note(key, start + Duration::from_millis(400));
        assert!(!due.take(key, start + COALESCE + ISSUE_SPACING / 2));
        assert!(due.take(key, start + COALESCE + ISSUE_SPACING));
    }

    #[test]
    fn a_pending_refresh_is_retired_with_its_view_revision() {
        let now = Instant::now();
        let current = (crate::hub::ScopeGeneration::default(), 1);
        let next = (current.0, 2);
        let mut due = Due::new(ISSUE_SPACING);
        due.note(current, now);
        assert!(!due.take(next, now + COALESCE));
        due.note(next, now);
        assert!(due.take(next, now + COALESCE));
    }
}
