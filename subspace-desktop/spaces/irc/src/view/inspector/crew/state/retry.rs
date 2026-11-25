//! Automatic retries for a failed crew detail read.

use std::time::Duration;

use subspace_client_fleet::Error;

const MAX_RETRIES: u32 = 5;
const MAX_RETRY_WAIT_SECS: u64 = 30;
const SERVER_ERRORS: std::ops::RangeInclusive<u16> = 500..=599;

fn retry_wait(error: Error) -> Option<u64> {
    match error {
        Error::Busy(wait) => Some(wait.unwrap_or_default()),
        Error::Unreachable => Some(0),
        Error::UnexpectedStatus(status) if SERVER_ERRORS.contains(&status) => Some(0),
        _ => None,
    }
}

pub(super) fn retry_delay(error: Error, retries: u32) -> Option<Duration> {
    let wait = retry_wait(error)?;
    // Don't automatically retry earlier than the gateway asked us to.
    (retries < MAX_RETRIES && wait <= MAX_RETRY_WAIT_SECS)
        .then(|| Duration::from_secs(wait.max(1 << retries)))
}
