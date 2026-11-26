//! A cancellation token and per-request retry budget for snapshot reads.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::Error;

const MAX_ATTEMPTS: usize = 3;
const MAX_WAIT_SECONDS: u64 = 10;
const DEFAULT_WAIT_SECONDS: u64 = 1;
const CANCEL_CHECK_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Default)]
pub struct DiffRead {
    cancelled: Arc<AtomicBool>,
    on_busy: Option<Arc<dyn Fn(u64) + Send + Sync>>,
    remaining_hold: Option<Arc<dyn Fn() -> (Instant, Duration) + Send + Sync>>,
}

impl DiffRead {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Cancel outstanding clones, keeping their hold hooks for future reads.
    pub fn cancel_and_reset(&mut self) {
        self.cancel();
        self.cancelled = Arc::default();
    }

    pub fn check(&self) -> Result<(), Error> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(Error::DiffReadCancelled)
        } else {
            Ok(())
        }
    }

    /// Observe positive Retry-After values before waiting or returning Busy.
    /// The observer also sees a reply if the request was cancelled in flight.
    pub fn observing_busy(mut self, observe: impl Fn(u64) + Send + Sync + 'static) -> Self {
        let prior = self.on_busy.take();
        self.on_busy = Some(Arc::new(move |seconds| {
            if let Some(prior) = &prior {
                prior(seconds);
            }
            observe(seconds);
        }));
        self
    }

    /// Read the clock and remaining hold for this request's identity before
    /// every send, so both the wait budget and the hold use the same clock.
    pub fn respecting_hold(
        mut self,
        remaining: impl Fn() -> (Instant, Duration) + Send + Sync + 'static,
    ) -> Self {
        self.remaining_hold = Some(Arc::new(remaining));
        self
    }

    /// Retry the identical request, never a newer cursor or fingerprint.
    /// If the server asks for longer than the budget, return Busy instead
    /// of retrying before the server's requested time.
    pub fn retry<T>(&self, mut request: impl FnMut() -> Result<T, Error>) -> Result<T, Error> {
        let mut attempt = || request().inspect_err(|error| self.observe_busy(error));
        let mut wait = Duration::ZERO;
        for _ in 1..MAX_ATTEMPTS {
            self.wait_before_send(wait)?;
            match attempt() {
                Err(Error::Busy(seconds)) => {
                    wait = Duration::from_secs(seconds.unwrap_or(DEFAULT_WAIT_SECONDS));
                }
                result => return result,
            }
        }
        self.wait_before_send(wait)?;
        attempt()
    }

    fn observe_busy(&self, error: &Error) {
        if let (Error::Busy(Some(seconds @ 1..)), Some(observe)) = (error, &self.on_busy) {
            observe(*seconds);
        }
    }

    fn wait_before_send(&self, reply_wait: Duration) -> Result<(), Error> {
        let mut started = None;
        let budget = Duration::from_secs(MAX_WAIT_SECONDS);
        loop {
            self.check()?;
            let (now, hold) = self
                .remaining_hold
                .as_ref()
                .map_or_else(|| (Instant::now(), Duration::ZERO), |read| read());
            let elapsed = now.saturating_duration_since(*started.get_or_insert(now));
            let delay = reply_wait.saturating_sub(elapsed).max(hold);
            if delay.is_zero() {
                return self.check();
            }
            // Compare the requested delay before sleeping; check again if a
            // late response extends the shared hold while we are waiting.
            if delay > budget.saturating_sub(elapsed) {
                let seconds = delay
                    .as_secs()
                    .saturating_add(u64::from(delay.subsec_nanos() != 0));
                return Err(Error::Busy(Some(seconds)));
            }
            std::thread::sleep(delay.min(CANCEL_CHECK_INTERVAL));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::{DiffRead, Error};

    #[test]
    fn gives_up_after_three_attempts_or_when_the_server_asks_for_a_long_wait() {
        for (wait, expected) in [(0, 3), (11, 1)] {
            let mut attempts = 0;
            let result: Result<(), _> = DiffRead::default().retry(|| {
                attempts += 1;
                Err(Error::Busy(Some(wait)))
            });
            assert_eq!(result, Err(Error::Busy(Some(wait))));
            assert_eq!(attempts, expected);
        }
    }

    #[test]
    fn cancellation_stops_a_busy_retry() {
        let read = DiffRead::default();
        let mut attempts = 0;
        let result: Result<(), _> = read.retry(|| {
            attempts += 1;
            read.cancel();
            Err(Error::Busy(Some(10)))
        });
        assert_eq!(result, Err(Error::DiffReadCancelled));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn returns_the_longer_shared_hold_without_sending() {
        let held = Arc::new(Mutex::new(Duration::ZERO));
        let store = held.clone();
        let read =
            DiffRead::default().respecting_hold(move || (Instant::now(), *store.lock().unwrap()));
        let result: Result<(), _> = read.retry(|| {
            *held.lock().unwrap() = Duration::from_millis(11_250);
            Err(Error::Busy(Some(3)))
        });
        assert_eq!(result, Err(Error::Busy(Some(12))));
        assert_eq!(read.retry(|| Ok(())), Err(Error::Busy(Some(12))));
        *held.lock().unwrap() = Duration::ZERO;
        assert_eq!(read.retry(|| Ok(())), Ok(()));
    }

    #[test]
    fn a_late_extension_cannot_restart_the_wait_budget() {
        let samples = AtomicUsize::new(0);
        let read = DiffRead::default().respecting_hold(move || {
            let hold = match samples.fetch_add(1, Ordering::Relaxed) {
                0 => Duration::from_millis(1),
                1 => Duration::from_secs(10),
                _ => Duration::ZERO,
            };
            (Instant::now(), hold)
        });
        assert_eq!(read.retry(|| Ok(())), Err(Error::Busy(Some(10))));
    }

    #[test]
    fn reports_positive_waits_before_backoff_and_on_the_last_attempt() {
        let read = DiffRead::default();
        let cancel = read.clone();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let out = seen.clone();
        let mut read = read.observing_busy(move |seconds| {
            out.lock().unwrap().push(seconds);
            cancel.cancel();
        });
        let result: Result<(), _> = read.retry(|| Err(Error::Busy(Some(10))));
        assert_eq!(result, Err(Error::DiffReadCancelled));
        read.cancel_and_reset();
        let mut waits = [Some(0), Some(0), Some(11)].into_iter();
        let result: Result<(), _> = read.retry(|| Err(Error::Busy(waits.next().unwrap())));
        assert_eq!(result, Err(Error::Busy(Some(11))));
        assert_eq!(*seen.lock().unwrap(), [10, 11]);
    }
}
