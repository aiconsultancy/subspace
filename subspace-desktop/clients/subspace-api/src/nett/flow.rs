//! How hard a NETT client leans on the API. There is no fixed quota per
//! window. Each account keeps at most [`CONCURRENT`] REST requests in flight,
//! counted from before the bearer is attached until the reply's body has
//! been read, so a request that waited is checked against the current
//! account and organisation before it is sent. Identical reads already in
//! flight are shared. A 429 or 503 pauses the account's requests for the
//! whole seconds in `Retry-After`, up to [`MAX_PAUSE`], or for a jittered
//! exponential back-off when it gives none; a request that would wait
//! longer than [`MAX_WAIT`] is refused at once rather than sent early. An
//! explicit reload, or choosing another organisation, ends a pause at once.
//! The update stream is separate: one per Issues hub, which follows one
//! organisation scope.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::Error;

/// REST requests one account may have in flight at once.
pub(crate) const CONCURRENT: usize = 6;
/// The longest a request waits for a pause to end before it is refused.
pub(crate) const MAX_WAIT: Duration = Duration::from_secs(30);
/// The longest back-off when a refusal names no wait.
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// The longest server wait honoured, as for comments. NETT, or anything in
/// front of it, may ask for longer, but a longer pause would leave the
/// account's reads dead for the rest of the session: no request would be
/// sent that could end it.
pub(crate) const MAX_PAUSE: Duration = Duration::from_secs(crate::writing::MAX_RETRY_SECONDS);
/// The first back-off when a refusal names no wait.
const FIRST_BACKOFF: Duration = Duration::from_secs(1);
/// Doublings before the back-off reaches [`MAX_BACKOFF`] (1 s × 2⁵ > 30 s).
const MAX_DOUBLINGS: u32 = 5;
/// At most this share of a back-off is taken off at random.
const MAX_JITTER_SHARE: f64 = 0.5;
/// Attempts a read makes when NETT is busy.
pub(crate) const READ_ATTEMPTS: usize = 3;

#[derive(Default)]
struct Account {
    active: usize,
    paused_until: Option<Instant>,
    refusals: u32,
}

#[derive(Default)]
struct Flows {
    accounts: HashMap<u64, Account>,
}

static FLOWS: Mutex<Option<Flows>> = Mutex::new(None);
static CHANGED: Condvar = Condvar::new();

fn flows() -> MutexGuard<'static, Option<Flows>> {
    FLOWS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn account_state<'a>(
    guard: &'a mut MutexGuard<'static, Option<Flows>>,
    account: u64,
) -> &'a mut Account {
    guard
        .get_or_insert_with(Flows::default)
        .accounts
        .entry(account)
        .or_default()
}

/// One request's place among its account's in-flight requests.
#[must_use]
pub(crate) struct Permit {
    account: u64,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut guard = flows();
        let state = account_state(&mut guard, self.account);
        state.active = state.active.saturating_sub(1);
        drop(guard);
        CHANGED.notify_all();
    }
}

/// Requests the account has in flight now.
#[cfg(test)]
pub(crate) fn active(account: u64) -> usize {
    account_state(&mut flows(), account).active
}

/// How long the account's requests are still paused.
pub(crate) fn paused_for(account: u64) -> Option<Duration> {
    let mut guard = flows();
    account_state(&mut guard, account)
        .paused_until
        .and_then(|until| until.checked_duration_since(Instant::now()))
        .filter(|wait| !wait.is_zero())
}

/// Waits for a free place and for a short pause to end, then takes the
/// place. A pause longer than [`MAX_WAIT`] refuses at once with what is
/// left of it.
pub(crate) fn acquire(account: u64) -> Result<Permit, Duration> {
    let mut guard = flows();
    loop {
        let now = Instant::now();
        let state = account_state(&mut guard, account);
        let wait = state
            .paused_until
            .and_then(|until| until.checked_duration_since(now))
            .filter(|wait| !wait.is_zero());
        match wait {
            Some(wait) if wait > MAX_WAIT => return Err(wait),
            Some(wait) => {
                guard = CHANGED
                    .wait_timeout(guard, wait)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
            None if state.active >= CONCURRENT => {
                guard = CHANGED.wait(guard).unwrap_or_else(PoisonError::into_inner);
            }
            None => {
                state.active += 1;
                return Ok(Permit { account });
            }
        }
    }
}

/// Records a response: a 429 or 503 pauses the account; anything else ends
/// the run of refusals.
pub(crate) fn observe(account: u64, status: u16, retry_after: Option<u64>) {
    let mut guard = flows();
    let state = account_state(&mut guard, account);
    if matches!(status, 429 | 503) {
        let pause = pause(state.refusals, retry_after, rand::random_range(0.0..=1.0));
        state.refusals = state.refusals.saturating_add(1);
        let until = deadline(Instant::now(), pause);
        state.paused_until = Some(state.paused_until.map_or(until, |at| at.max(until)));
    } else {
        state.refusals = 0;
    }
    drop(guard);
    CHANGED.notify_all();
}

/// When a server's wait of `pause` from `now` ends: never sooner, but no
/// later than [`MAX_PAUSE`] from now.
pub(crate) fn deadline(now: Instant, pause: Duration) -> Instant {
    now + pause.min(MAX_PAUSE)
}

/// Ends the account's pause at once, after an explicit reload or a change
/// of organisation: the person asked for the requests to be sent.
pub(crate) fn resume(account: u64) {
    let mut guard = flows();
    let state = account_state(&mut guard, account);
    state.paused_until = None;
    state.refusals = 0;
    drop(guard);
    CHANGED.notify_all();
}

/// The pause after a refusal: the server's whole seconds when given, up to
/// [`MAX_PAUSE`], or else an exponential back-off from one second, up to
/// 30, with up to half of it taken off at random.
pub(crate) fn pause(refusals: u32, retry_after: Option<u64>, jitter: f64) -> Duration {
    if let Some(seconds) = retry_after {
        return Duration::from_secs(seconds).min(MAX_PAUSE);
    }
    let full = FIRST_BACKOFF
        .saturating_mul(1u32 << refusals.min(MAX_DOUBLINGS))
        .min(MAX_BACKOFF);
    full.mul_f64(1.0 - jitter.clamp(0.0, 1.0) * MAX_JITTER_SHARE)
}

/// A `Retry-After` in whole seconds, as NETT sends it, up to five minutes.
pub(crate) fn retry_after(response: &ureq::http::Response<ureq::Body>) -> Option<u64> {
    crate::transport::header(response, "retry-after")
        .trim()
        .parse::<u64>()
        .ok()
        .map(|seconds| seconds.min(MAX_PAUSE.as_secs()))
}

/// The answer to a request refused before it was sent, because the account
/// is paused for longer than a request waits.
pub(crate) fn refused(wait: Duration) -> ureq::http::Response<ureq::Body> {
    let mut response = ureq::http::Response::new(ureq::Body::builder().data(Vec::<u8>::new()));
    *response.status_mut() = ureq::http::StatusCode::TOO_MANY_REQUESTS;
    let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
    if let Ok(value) = seconds.to_string().parse() {
        response.headers_mut().insert("retry-after", value);
    }
    response
}

/// Keeps the request's place until its body has been read or dropped, and
/// records a refusal before the place is given up.
pub(crate) fn hold(
    account: u64,
    permit: Permit,
    response: ureq::http::Response<ureq::Body>,
) -> ureq::http::Response<ureq::Body> {
    observe(account, response.status().as_u16(), retry_after(&response));
    let (parts, body) = response.into_parts();
    let mut builder = ureq::Body::builder();
    if let Some(mime) = body.mime_type() {
        builder = builder.mime_type(mime);
    }
    if let Some(charset) = body.charset() {
        builder = builder.charset(charset);
    }
    let body = builder.reader(Held {
        inner: Mutex::new(Box::new(body.into_reader())),
        _permit: permit,
    });
    ureq::http::Response::from_parts(parts, body)
}

struct Held {
    inner: Mutex<Box<dyn Read + Send>>,
    _permit: Permit,
}

impl Read for Held {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .read(buf)
    }
}

/// A ureq middleware step for a loopback NETT client: a place among the
/// account's requests, held until the reply's body is read.
pub(crate) fn through(
    account: u64,
    request: ureq::http::Request<ureq::SendBody>,
    next: ureq::middleware::MiddlewareNext,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let permit = match acquire(account) {
        Ok(permit) => permit,
        Err(wait) => return Ok(refused(wait)),
    };
    next.handle(request)
        .map(|response| hold(account, permit, response))
}

type ReadKey = (u64, String);
type Shared = Arc<(Mutex<Option<Result<Value, Error>>>, Condvar)>;

static READS: Mutex<Option<HashMap<ReadKey, Shared>>> = Mutex::new(None);

fn reads() -> MutexGuard<'static, Option<HashMap<ReadKey, Shared>>> {
    READS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `read` once for every caller asking for the same account and URL at
/// the same time; the others wait and share its answer.
pub(crate) fn coalesce(
    account: u64,
    url: &str,
    read: impl FnOnce() -> Result<Value, Error>,
) -> Result<Value, Error> {
    let key = (account, url.to_owned());
    let (shared, leader) = {
        let mut guard = reads();
        let table = guard.get_or_insert_with(HashMap::new);
        match table.get(&key) {
            Some(shared) => (shared.clone(), false),
            None => {
                let shared: Shared = Arc::new((Mutex::new(None), Condvar::new()));
                table.insert(key.clone(), shared.clone());
                (shared, true)
            }
        }
    };
    let (slot, ready) = &*shared;
    if !leader {
        let mut answer = slot.lock().unwrap_or_else(PoisonError::into_inner);
        while answer.is_none() {
            answer = ready.wait(answer).unwrap_or_else(PoisonError::into_inner);
        }
        return answer.clone().unwrap_or(Err(Error::Network));
    }
    // The entry leaves the table even if the read panics, so later callers
    // read afresh instead of waiting forever.
    struct Leave(Option<ReadKey>, Shared);
    impl Drop for Leave {
        fn drop(&mut self) {
            if let Some(key) = self.0.take() {
                if let Some(table) = reads().as_mut() {
                    table.remove(&key);
                }
                let (slot, ready) = &*self.1;
                let mut answer = slot.lock().unwrap_or_else(PoisonError::into_inner);
                if answer.is_none() {
                    *answer = Some(Err(Error::Network));
                }
                ready.notify_all();
            }
        }
    }
    let leave = Leave(Some(key), shared.clone());
    let result = read();
    *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(result.clone());
    drop(leave);
    result
}

#[cfg(test)]
mod tests {
    use super::{
        CONCURRENT, MAX_PAUSE, MAX_WAIT, acquire, coalesce, deadline, observe, pause, paused_for,
        resume,
    };
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    #[test]
    fn a_refusal_waits_for_the_servers_seconds_or_backs_off_with_jitter() {
        assert_eq!(pause(0, Some(2), 0.5), Duration::from_secs(2));
        // A long wait is honoured, never shortened, up to five minutes.
        assert_eq!(pause(3, Some(120), 0.0), Duration::from_secs(120));
        assert_eq!(pause(0, None, 0.0), Duration::from_secs(1));
        assert_eq!(pause(2, None, 0.0), Duration::from_secs(4));
        assert_eq!(pause(2, None, 1.0), Duration::from_secs(2));
        assert_eq!(pause(30, None, 0.0), Duration::from_secs(30));
        // Longer than five minutes is held for five minutes.
        assert_eq!(pause(0, Some(86_400), 0.0), MAX_PAUSE);
        let jittered = pause(1, None, 0.25);
        assert!(jittered > Duration::from_millis(1700) && jittered < Duration::from_secs(2));
    }

    #[test]
    fn an_account_keeps_a_few_requests_in_flight_and_others_wait() {
        let account = 0x5eed_0001;
        let held: Vec<_> = (0..CONCURRENT).map(|_| acquire(account).unwrap()).collect();
        let started = Arc::new(AtomicUsize::new(0));
        let waiter = {
            let started = started.clone();
            std::thread::spawn(move || {
                let _permit = acquire(account).unwrap();
                started.fetch_add(1, Ordering::SeqCst);
            })
        };
        // Another account is not held up.
        drop(acquire(account + 1).unwrap());
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(started.load(Ordering::SeqCst), 0);
        drop(held);
        waiter.join().unwrap();
        assert_eq!(started.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_busy_answer_pauses_the_accounts_next_request() {
        let account = 0x5eed_0002;
        observe(account, 429, Some(1));
        let start = Instant::now();
        drop(acquire(account).unwrap());
        assert!(start.elapsed() >= Duration::from_millis(900));
        observe(account, 200, None);
        let start = Instant::now();
        drop(acquire(account).unwrap());
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn a_place_is_held_until_the_reply_body_is_read() {
        let account = 0x5eed_0004;
        let reply = || {
            let permit = acquire(account).unwrap();
            let response =
                ureq::http::Response::new(ureq::Body::builder().data(b"{\"items\":[]}".to_vec()));
            super::hold(account, permit, response)
        };
        // Six replies whose bodies are not read yet hold all six places.
        let mut pending: Vec<_> = (0..CONCURRENT).map(|_| reply()).collect();
        let started = Arc::new(AtomicUsize::new(0));
        let waiter = {
            let started = started.clone();
            std::thread::spawn(move || {
                let _permit = acquire(account).unwrap();
                started.fetch_add(1, Ordering::SeqCst);
            })
        };
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            started.load(Ordering::SeqCst),
            0,
            "headers alone do not free a place"
        );
        let body = pending.pop().unwrap().into_body().read_to_string().unwrap();
        assert_eq!(body, "{\"items\":[]}");
        waiter.join().unwrap();
        assert_eq!(started.load(Ordering::SeqCst), 1);
        drop(pending);
    }

    #[test]
    fn a_refusal_is_recorded_before_its_place_is_given_up() {
        let account = 0x5eed_0005;
        let permit = acquire(account).unwrap();
        let mut response = ureq::http::Response::new(ureq::Body::builder().data(Vec::<u8>::new()));
        *response.status_mut() = ureq::http::StatusCode::SERVICE_UNAVAILABLE;
        response
            .headers_mut()
            .insert("retry-after", "40".parse().unwrap());
        let held = super::hold(account, permit, response);
        assert!(paused_for(account).is_some_and(|wait| wait > MAX_WAIT));
        drop(held);
        assert!(
            acquire(account).is_err(),
            "the next request is refused, not sent"
        );
    }

    #[test]
    fn a_day_long_wait_pauses_the_account_for_five_minutes_at_most() {
        // A 503 asking for a day, or more than the clock holds, from NETT or
        // anything in front of it, does not leave the account dead.
        for (account, seconds) in [(0x5eed_0006, 86_400), (0x5eed_0007, u64::MAX)] {
            observe(account, 503, Some(seconds));
            let paused = paused_for(account).unwrap();
            assert!(paused <= MAX_PAUSE && paused > MAX_PAUSE - Duration::from_secs(5));
        }
        // A reload or another organisation ends the pause at once.
        resume(0x5eed_0006);
        assert_eq!(paused_for(0x5eed_0006), None);
        assert!(acquire(0x5eed_0006).is_ok());
    }

    #[test]
    fn a_deadline_is_never_sooner_and_never_past_five_minutes() {
        let now = Instant::now();
        assert_eq!(deadline(now, Duration::MAX), now + MAX_PAUSE);
        assert_eq!(
            deadline(now, Duration::from_secs(90)),
            now + Duration::from_secs(90)
        );
    }

    #[test]
    fn a_wait_longer_than_a_request_waits_refuses_at_once_and_is_not_shortened() {
        let account = 0x5eed_0003;
        observe(account, 503, Some(45));
        let start = Instant::now();
        let left = acquire(account).err().unwrap();
        assert!(
            start.elapsed() < Duration::from_millis(200),
            "no early send, no long block"
        );
        assert!(left > MAX_WAIT && left <= Duration::from_secs(45));
        assert!(paused_for(account).unwrap() > MAX_WAIT);
    }

    #[test]
    fn identical_reads_in_flight_share_one_request() {
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(4));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let (calls, barrier) = (calls.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    coalesce(7, "http://127.0.0.1:1/api/v1/orgs/o/issues", || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(150));
                        Ok(json!({"items": []}))
                    })
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap().unwrap(), json!({"items": []}));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Another account, or a later read, asks again.
        let again = coalesce(8, "http://127.0.0.1:1/api/v1/orgs/o/issues", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(json!(1))
        });
        assert_eq!(again.unwrap(), json!(1));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
