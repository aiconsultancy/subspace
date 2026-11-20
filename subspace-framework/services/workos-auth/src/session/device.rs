//! WorkOS device authorisation: validate the device code reply, then poll
//! for the result at the interval WorkOS asks for.

use crate::http::{DevicePoll, DeviceStart, REQUEST_TIMEOUT, Reply, Transport};
use std::time::{Duration, Instant};
use subspace_account::{Cancel, Error};
use subspace_core::Secret;

const GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const MAX_DEVICE_CODE: usize = 8192;
const MAX_USER_CODE: usize = 128;
const MAX_URL: usize = 2048;
const MAX_EXPIRY_SECONDS: u64 = 3600;
const DEFAULT_INTERVAL_SECONDS: u64 = 5;
const MAX_INTERVAL_SECONDS: u64 = 60;
/// RFC 8628: each `slow_down` adds five seconds to the interval.
const SLOW_DOWN_SECONDS: u64 = 5;
/// Consecutive unreachable or failed polls before giving up.
const MAX_FAILED_POLLS: u32 = 3;

/// How often a wait checks for cancellation.
const SLICE: Duration = Duration::from_millis(100);

/// Time for the device protocol. Tests swap in a virtual clock, so waits
/// and deadlines are exact without sleeping.
pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, duration: Duration);
}

/// The real clock.
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// One validated device authorisation, kept inside the provider.
pub(super) struct Device {
    code: Secret,
    interval: Duration,
    deadline: Instant,
}

/// The prompt the shell shows: the page to open and the code to confirm.
pub(super) struct Prompt {
    pub url: String,
    pub user_code: String,
}

pub(super) fn validate(
    start: DeviceStart,
    issued: Instant,
    loopback: bool,
) -> Result<(Prompt, Device), Error> {
    let interval = start.interval.unwrap_or(DEFAULT_INTERVAL_SECONDS);
    let code = start.device_code.expose();
    if code.trim().is_empty()
        || code.len() > MAX_DEVICE_CODE
        || start.user_code.trim().is_empty()
        || start.user_code.len() > MAX_USER_CODE
        || start.user_code.chars().any(char::is_control)
        || !(1..=MAX_EXPIRY_SECONDS).contains(&start.expires_in)
        || !(1..=MAX_INTERVAL_SECONDS).contains(&interval)
    {
        return Err(Error::InvalidReply);
    }
    let url = prompt_url(&start, loopback)?;
    Ok((
        Prompt {
            url,
            user_code: start.user_code,
        },
        Device {
            code: start.device_code,
            interval: Duration::from_secs(interval),
            deadline: issued + Duration::from_secs(start.expires_in),
        },
    ))
}

/// The complete verification page when it names exactly this user code on
/// the same page, otherwise the plain page.
fn prompt_url(start: &DeviceStart, loopback: bool) -> Result<String, Error> {
    let page = verification_url(&start.verification_uri, loopback)?;
    if page.query().is_some() {
        return Err(Error::InvalidReply);
    }
    let Some(complete) = &start.verification_uri_complete else {
        return Ok(page.into());
    };
    let complete = verification_url(complete, loopback)?;
    let pairs: Vec<_> = complete.query_pairs().collect();
    if complete.origin() != page.origin()
        || complete.path() != page.path()
        || pairs.len() != 1
        || pairs[0].0 != "user_code"
        || pairs[0].1 != start.user_code
    {
        return Err(Error::InvalidReply);
    }
    Ok(complete.into())
}

/// HTTPS without credentials or a fragment; plain HTTP only on a loopback
/// address when the WorkOS API itself is a loopback stand-in.
fn verification_url(value: &str, loopback: bool) -> Result<url::Url, Error> {
    let url = url::Url::parse(value).map_err(|_| Error::InvalidReply)?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    let secure = url.scheme() == "https" || (loopback && local && url.scheme() == "http");
    if value.len() > MAX_URL
        || !secure
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidReply);
    }
    Ok(url)
}

/// Poll until WorkOS grants tokens, the code expires, access is refused,
/// or the sign-in is cancelled. No exchange outlives the code: each request
/// is capped by its remaining lifetime, and a reply arriving after the
/// deadline counts as expired, tokens included.
pub(super) fn poll(
    transport: &dyn Transport,
    clock: &dyn Clock,
    url: &str,
    client_id: &str,
    device: &Device,
    cancel: &Cancel,
) -> Result<Reply, Error> {
    let mut interval = device.interval;
    let mut failures = 0;
    loop {
        wait(clock, interval, device.deadline, cancel)?;
        let timeout = exchange_timeout(clock.now(), device.deadline)?;
        let form = [
            ("grant_type", GRANT),
            ("device_code", device.code.expose()),
            ("client_id", client_id),
        ];
        let outcome = transport.poll_device(url, &form, timeout);
        if clock.now() >= device.deadline {
            return Err(Error::TimedOut);
        }
        match outcome {
            Ok(DevicePoll::Granted(reply)) => return Ok(*reply),
            Ok(DevicePoll::Pending) => failures = 0,
            Ok(DevicePoll::SlowDown) => {
                failures = 0;
                interval = (interval + Duration::from_secs(SLOW_DOWN_SECONDS))
                    .min(Duration::from_secs(MAX_INTERVAL_SECONDS));
            }
            Err(error @ (Error::Offline | Error::Service)) => {
                failures += 1;
                if failures >= MAX_FAILED_POLLS {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
}

/// The longest one exchange may take: the transport's usual limit, or the
/// code's remaining lifetime when that is shorter.
fn exchange_timeout(now: Instant, deadline: Instant) -> Result<Duration, Error> {
    let remaining = deadline.saturating_duration_since(now);
    if remaining.is_zero() {
        return Err(Error::TimedOut);
    }
    Ok(remaining.min(REQUEST_TIMEOUT))
}

fn wait(
    clock: &dyn Clock,
    interval: Duration,
    deadline: Instant,
    cancel: &Cancel,
) -> Result<(), Error> {
    let until = clock.now() + interval;
    if until >= deadline {
        return Err(Error::TimedOut);
    }
    loop {
        cancel.check()?;
        let now = clock.now();
        if now >= until {
            return Ok(());
        }
        clock.sleep(SLICE.min(until - now));
    }
}

/// A clock that moves only when something sleeps or advances it.
#[cfg(test)]
pub(crate) struct VirtualClock {
    start: Instant,
    elapsed: std::sync::Mutex<Duration>,
}

#[cfg(test)]
impl VirtualClock {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
            elapsed: std::sync::Mutex::default(),
        }
    }

    pub(crate) fn advance(&self, duration: Duration) {
        *self.elapsed.lock().unwrap() += duration;
    }

    /// Time since the clock was made.
    pub(crate) fn elapsed(&self) -> Duration {
        *self.elapsed.lock().unwrap()
    }
}

#[cfg(test)]
impl Clock for VirtualClock {
    fn now(&self) -> Instant {
        self.start + self.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        self.advance(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock as _, DeviceStart, VirtualClock, poll, validate};
    use crate::http::{DevicePoll, Reply, Transport, User};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use subspace_account::{Cancel, Error};
    use subspace_core::Secret;

    /// Answers polls from a script, noting when each arrived and the
    /// timeout it was given. A poll may take time on the virtual clock.
    struct Scripted {
        clock: Arc<VirtualClock>,
        answers: Mutex<Vec<(Duration, DevicePoll)>>,
        polls: Mutex<Vec<(u64, u64)>>,
    }

    impl Transport for Scripted {
        fn authenticate(&self, _: &str, _: serde_json::Value) -> Result<Reply, Error> {
            Err(Error::Service)
        }

        fn poll_device(
            &self,
            _: &str,
            _: &[(&str, &str)],
            timeout: Duration,
        ) -> Result<DevicePoll, Error> {
            self.polls
                .lock()
                .unwrap()
                .push((self.clock.elapsed().as_secs(), timeout.as_secs()));
            let mut answers = self.answers.lock().unwrap();
            if answers.is_empty() {
                return Ok(DevicePoll::Pending);
            }
            let (takes, answer) = answers.remove(0);
            self.clock.advance(takes.min(timeout));
            Ok(answer)
        }
    }

    fn tokens() -> DevicePoll {
        DevicePoll::Granted(Box::new(Reply {
            user: User {
                id: "user_fixture".into(),
                email: String::new(),
                first_name: None,
                last_name: None,
            },
            organization_id: None,
            access_token: Secret::new("access"),
            refresh_token: Secret::new("refresh"),
        }))
    }

    /// Polls a code issued now, lasting `expires_in` seconds with a
    /// five-second interval.
    fn run(
        expires_in: u64,
        answers: Vec<(Duration, DevicePoll)>,
    ) -> (Result<Reply, Error>, Vec<(u64, u64)>) {
        let clock = Arc::new(VirtualClock::new());
        let mut code = start("https://auth.example.com/device", None);
        code.expires_in = expires_in;
        let (_, device) = validate(code, clock.now(), false).unwrap();
        let transport = Scripted {
            clock: clock.clone(),
            answers: Mutex::new(answers),
            polls: Mutex::default(),
        };
        let result = poll(
            &transport,
            clock.as_ref(),
            "https://auth.example.com/user_management/authenticate",
            "client_fixture",
            &device,
            &Cancel::default(),
        );
        (result, transport.polls.into_inner().unwrap())
    }

    #[test]
    fn polls_wait_the_interval_and_five_seconds_more_after_each_slow_down() {
        let now = Duration::ZERO;
        let (result, polls) = run(
            300,
            vec![
                (now, DevicePoll::Pending),
                (now, DevicePoll::SlowDown),
                (now, DevicePoll::Pending),
                (now, DevicePoll::SlowDown),
                (now, tokens()),
            ],
        );
        assert!(result.is_ok());
        let at: Vec<_> = polls.iter().map(|(at, _)| *at).collect();
        assert_eq!(at, [5, 10, 20, 30, 45]);
    }

    #[test]
    fn each_exchange_is_capped_by_the_codes_remaining_lifetime() {
        let (result, polls) = run(32, Vec::new());
        assert_eq!(result.err(), Some(Error::TimedOut));
        assert_eq!(
            polls,
            [(5, 20), (10, 20), (15, 17), (20, 12), (25, 7), (30, 2)]
        );
    }

    #[test]
    fn tokens_that_arrive_after_the_code_expires_are_refused() {
        // The second exchange starts two seconds before expiry and takes
        // longer than that: its tokens arrive too late to use.
        let (result, polls) = run(
            12,
            vec![
                (Duration::ZERO, DevicePoll::Pending),
                (Duration::from_secs(20), tokens()),
            ],
        );
        assert_eq!(result.err(), Some(Error::TimedOut));
        assert_eq!(polls, [(5, 7), (10, 2)]);
        // Just inside the deadline, the same tokens are accepted.
        let (result, _) = run(
            12,
            vec![
                (Duration::ZERO, DevicePoll::Pending),
                (Duration::from_secs(1), tokens()),
            ],
        );
        assert!(result.is_ok());
    }

    fn start(page: &str, complete: Option<&str>) -> DeviceStart {
        DeviceStart {
            device_code: Secret::new("device-fixture"),
            user_code: "ABCD-EFGH".into(),
            verification_uri: page.into(),
            verification_uri_complete: complete.map(str::to_owned),
            expires_in: 300,
            interval: Some(5),
        }
    }

    #[test]
    fn the_prompt_is_the_complete_page_for_this_code_on_the_same_page() {
        let (prompt, _) = validate(
            start(
                "https://auth.example.com/device",
                Some("https://auth.example.com/device?user_code=ABCD-EFGH"),
            ),
            Instant::now(),
            false,
        )
        .unwrap();
        assert_eq!(
            prompt.url,
            "https://auth.example.com/device?user_code=ABCD-EFGH"
        );
        assert_eq!(prompt.user_code, "ABCD-EFGH");
        let (plain, _) = validate(
            start("https://auth.example.com/device", None),
            Instant::now(),
            false,
        )
        .unwrap();
        assert_eq!(plain.url, "https://auth.example.com/device");
    }

    #[test]
    fn unsafe_or_mismatched_verification_pages_are_refused() {
        for (page, complete) in [
            ("http://auth.example.com/device", None),
            ("https://user@auth.example.com/device", None),
            ("https://auth.example.com/device#x", None),
            ("https://auth.example.com/device?x=1", None),
            (
                "https://auth.example.com/device",
                Some("https://other.example.com/device?user_code=ABCD-EFGH"),
            ),
            (
                "https://auth.example.com/device",
                Some("https://auth.example.com/device?user_code=WXYZ-0000"),
            ),
            ("http://127.0.0.1:9000/device", None),
        ] {
            assert_eq!(
                validate(start(page, complete), Instant::now(), false).err(),
                Some(Error::InvalidReply),
                "{page}"
            );
        }
        assert!(
            validate(
                start("http://127.0.0.1:9000/device", None),
                Instant::now(),
                true
            )
            .is_ok()
        );
        let mut bad = start("https://auth.example.com/device", None);
        bad.interval = Some(0);
        assert!(validate(bad, Instant::now(), false).is_err());
        let mut bad = start("https://auth.example.com/device", None);
        bad.user_code = "line\nbreak".into();
        assert!(validate(bad, Instant::now(), false).is_err());
    }
}
