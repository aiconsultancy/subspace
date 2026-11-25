//! Automatic retries and gateway-directed holds for failed crew detail reads.

use std::time::Duration;

use subspace_client_fleet::Error;

const MAX_RETRIES: u32 = 5;
const MAX_RETRY_WAIT_SECS: u64 = 30;
const SERVER_ERRORS: std::ops::RangeInclusive<u16> = 500..=599;

#[derive(Clone, Copy)]
pub(super) enum ReadWait {
    Retry(Duration),
    // Releases queued demand without creating a new read or using retry budget.
    Hold(Duration),
}

impl ReadWait {
    pub(super) fn after(error: Error, retries: u32, roster_present: bool) -> Option<Self> {
        if roster_present && let Some(delay) = retry_delay(error, retries) {
            return Some(Self::Retry(delay));
        }
        match error {
            Error::Busy(Some(seconds)) => Some(Self::Hold(Duration::from_secs(seconds))),
            _ => None,
        }
    }

    pub(super) fn delay(self) -> Duration {
        match self {
            Self::Retry(delay) | Self::Hold(delay) => delay,
        }
    }
}

fn retry_wait(error: Error) -> Option<u64> {
    match error {
        Error::Busy(wait) => Some(wait.unwrap_or_default()),
        Error::Unreachable => Some(0),
        Error::UnexpectedStatus(status) if SERVER_ERRORS.contains(&status) => Some(0),
        _ => None,
    }
}

fn retry_delay(error: Error, retries: u32) -> Option<Duration> {
    let wait = retry_wait(error)?;
    // Don't automatically retry earlier than the gateway asked us to.
    (retries < MAX_RETRIES && wait <= MAX_RETRY_WAIT_SECS)
        .then(|| Duration::from_secs(wait.max(1 << retries)))
}
