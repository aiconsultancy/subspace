//! Whether the account side's requests reach the API: a check or a login
//! request that cannot reach it at all is named in Channels' status while
//! it is tried again, and Reload asks again at once.

use std::time::{Duration, Instant};

use subspace_api_client::Error;

use crate::managed::Managed;
use crate::managed::account::{Checked, Person};

fn checked(result: Result<Option<Person>, Error>) -> Checked {
    Checked {
        client: None,
        generation: None,
        result,
    }
}

#[test]
fn a_check_that_cannot_reach_the_api_is_named_and_tried_again() {
    let mut managed = Managed::default();
    assert!(!managed.unreachable());
    let now = Instant::now();
    managed.finish(checked(Err(Error::Network)), now);
    assert!(managed.unreachable());
    assert!(!managed.schedule.due(now), "it waits before the retry");
    assert!(
        managed.schedule.due(now + Duration::from_secs(5)),
        "and is checked again"
    );
    managed.finish(checked(Err(Error::Forbidden)), now);
    assert!(!managed.unreachable(), "a refusal reached it");
    managed.finish(checked(Err(Error::Network)), now);
    managed.forget();
    assert!(!managed.unreachable(), "another account starts afresh");
}

#[test]
fn reload_checks_again_at_once_while_no_check_settled() {
    let mut managed = Managed::default();
    let now = Instant::now();
    managed.finish(checked(Err(Error::Network)), now);
    assert!(!managed.schedule.due(now));
    managed.retry_now();
    assert!(managed.schedule.due(now), "due now, not after the backoff");
    managed.finish(checked(Ok(None)), now);
    managed.retry_now();
    assert!(!managed.schedule.due(now), "a settled check stays settled");
}
