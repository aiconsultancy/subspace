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
pub struct DiffRead(Arc<AtomicBool>);

impl DiffRead {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn check(&self) -> Result<(), Error> {
        if self.0.load(Ordering::Acquire) {
            Err(Error::DiffReadCancelled)
        } else {
            Ok(())
        }
    }

    /// Retry the identical request, never a newer cursor or fingerprint.
    /// If the server asks for longer than the budget, return Busy instead
    /// of retrying before the server's requested time.
    pub fn retry<T>(&self, mut request: impl FnMut() -> Result<T, Error>) -> Result<T, Error> {
        for _ in 1..MAX_ATTEMPTS {
            self.check()?;
            match request() {
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
        self.check()?;
        request()
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
}
