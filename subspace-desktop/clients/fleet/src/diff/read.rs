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
}

impl DiffRead {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Cancel outstanding clones, keeping the observer for future reads.
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

    /// Retry the identical request, never a newer cursor or fingerprint.
    /// If the server asks for longer than the budget, return Busy instead
    /// of retrying before the server's requested time.
    pub fn retry<T>(&self, mut request: impl FnMut() -> Result<T, Error>) -> Result<T, Error> {
        let mut attempt = || {
            self.check()?;
            request().inspect_err(|error| {
                if let (Error::Busy(Some(seconds @ 1..)), Some(observe)) = (error, &self.on_busy) {
                    observe(*seconds);
                }
            })
        };
        for _ in 1..MAX_ATTEMPTS {
            match attempt() {
                Err(error @ Error::Busy(wait)) => {
                    let seconds = wait.unwrap_or(DEFAULT_WAIT_SECONDS);
                    if seconds > MAX_WAIT_SECONDS {
                        return Err(error);
                    }
                    self.wait(Duration::from_secs(seconds))?;
                }
                result => return result,
            }
        }
        attempt()
    }

    fn wait(&self, wait: Duration) -> Result<(), Error> {
        let deadline = Instant::now() + wait;
        loop {
            self.check()?;
            let delay = deadline.saturating_duration_since(Instant::now());
            if delay.is_zero() {
                return Ok(());
            }
            std::thread::sleep(delay.min(CANCEL_CHECK_INTERVAL));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

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
