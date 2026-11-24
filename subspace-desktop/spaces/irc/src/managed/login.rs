//! One server's issued login: asked for with that server's host and port,
//! kept in memory until shortly before it expires, and never asked for
//! again sooner than a minimum interval, or a growing wait after a failure.
//! A login is only ever offered to the server it was issued for. Its
//! lifetime, by the API's clock and never more than ten minutes, runs from
//! when it was asked for on the monotonic clock, so neither a fast or slow
//! local clock nor a slow answer lets it be used past its expiry.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use subspace_api_client::{Client, Error, IrcCredential};
use subspace_framework::core::Secret;

use crate::initiatives::Endpoint;

/// A kept login is asked for again this long before it expires.
const RENEW_BEFORE: Duration = Duration::from_secs(60);
/// The least time between two requests for one server.
const MIN_INTERVAL: Duration = Duration::from_secs(10);
/// The first wait after a failed request; each further failure doubles it.
const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);
const RETRY_MAX_DOUBLINGS: u32 = 6;

/// A login for one connect to the server.
#[derive(Clone)]
pub(crate) struct Login {
    pub account: String,
    pub password: Secret,
    /// When it stops being accepted, on this computer's monotonic clock.
    pub valid_until: Instant,
}

/// Why no login is offered now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    /// The API refused or failed; [`Error::NotFound`] means it does not
    /// vouch for this server.
    Api(Error),
    /// The API answered with a login that had already expired.
    Expired,
    /// It is too soon to ask again.
    Waiting,
}

/// The login of the server at `host` and `port`, shared by its probe and
/// its connection's credential source.
pub(crate) struct ServerLogin {
    host: String,
    port: u16,
    held: Mutex<Held>,
}

#[derive(Default)]
struct Held {
    kept: Option<Login>,
    asked_at: Option<Instant>,
    /// A request is under way; its answer is taken when it comes.
    asking: bool,
    failures: u32,
    retry_at: Option<Instant>,
}

/// What to do for a connect at a given moment.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// Offer the kept login.
    Keep,
    /// Ask the API for a new one.
    Ask,
    /// Neither: it is too soon to ask and nothing usable is kept.
    Wait,
}

impl ServerLogin {
    pub(crate) fn new(endpoint: &Endpoint) -> Self {
        Self {
            host: endpoint.host.clone(),
            port: endpoint.port,
            held: Mutex::new(Held::default()),
        }
    }

    /// The server, as `host:port`.
    pub(crate) fn server(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// The login for a connect now: the kept one while it is fresh, else a
    /// new one when the API may be asked. Blocking, without holding the
    /// lock while the API answers, so [`Self::forget`] never waits on it.
    pub(crate) fn login(&self, client: &Client) -> Result<Login, Unavailable> {
        self.ask(client).0
    }

    /// [`Self::login`], with whether a request to the API reached it:
    /// `None` when none was made, as when the kept login is offered.
    pub(crate) fn ask(&self, client: &Client) -> (Result<Login, Unavailable>, Option<bool>) {
        let now = Instant::now();
        {
            let mut held = self.held();
            match held.step(now) {
                Step::Keep => return (held.login(now).ok_or(Unavailable::Waiting), None),
                Step::Wait => return (Err(Unavailable::Waiting), None),
                Step::Ask => held.begin(now),
            }
        }
        let answer = client.irc_credential(&self.host, self.port);
        let reached = !matches!(answer, Err(Error::Network));
        (self.held().take(answer, now, Instant::now()), Some(reached))
    }

    /// Lets the next request go now, as Reload asks, rather than after the
    /// wait a failure set. The shortest gap between requests still holds.
    pub(crate) fn retry_now(&self) {
        self.held().retry_at = None;
    }

    /// The server refused the kept login: drop it, so the next connect
    /// asks again once that is allowed.
    pub(crate) fn forget(&self) {
        let mut held = self.held();
        held.kept = None;
        held.fail(Instant::now());
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Held {
    fn step(&self, now: Instant) -> Step {
        let fresh = |kept: &Login| now + RENEW_BEFORE < kept.valid_until;
        if self.kept.as_ref().is_some_and(fresh) {
            return Step::Keep;
        }
        let too_soon = self.asking
            || self.retry_at.is_some_and(|at| now < at)
            || self
                .asked_at
                .is_some_and(|at| now.saturating_duration_since(at) < MIN_INTERVAL);
        match (too_soon, self.login(now)) {
            (false, _) => Step::Ask,
            // Too soon to renew, but the kept login has not expired yet.
            (true, Some(_)) => Step::Keep,
            (true, None) => Step::Wait,
        }
    }

    /// The kept login, while it has not expired at `now`.
    fn login(&self, now: Instant) -> Option<Login> {
        self.kept
            .as_ref()
            .filter(|kept| now < kept.valid_until)
            .cloned()
    }

    fn begin(&mut self, now: Instant) {
        self.asking = true;
        self.asked_at = Some(now);
    }

    /// Takes the API's `answer` to a request made at `asked` and received
    /// at `received`. A failed renewal keeps offering the kept login while
    /// it has not expired.
    fn take(
        &mut self,
        answer: Result<IrcCredential, Error>,
        asked: Instant,
        received: Instant,
    ) -> Result<Login, Unavailable> {
        self.asking = false;
        let unavailable = match answer {
            Ok(credential) if !credential.valid_for.is_zero() => {
                return Ok(self.keep(credential, asked));
            }
            Ok(_) => Unavailable::Expired,
            Err(error) => Unavailable::Api(error),
        };
        self.fail(asked);
        self.login(received).ok_or(unavailable)
    }

    /// Keeps `credential`, lasting from `asked`, when its request was sent.
    fn keep(&mut self, credential: IrcCredential, asked: Instant) -> Login {
        self.failures = 0;
        self.retry_at = None;
        let login = Login {
            account: credential.account,
            password: credential.password,
            valid_until: asked + credential.valid_for,
        };
        self.kept = Some(login.clone());
        login
    }

    fn fail(&mut self, now: Instant) {
        let doublings = self.failures.min(RETRY_MAX_DOUBLINGS);
        self.failures = self.failures.saturating_add(1);
        self.retry_at = Some(now + RETRY_FIRST.saturating_mul(1 << doublings).min(RETRY_MAX));
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use subspace_api_client::{Error, IrcCredential};
    use subspace_framework::core::Secret;

    use super::{Held, Step, Unavailable};

    fn credential(valid_for: Duration) -> Result<IrcCredential, Error> {
        Ok(IrcCredential {
            account: "ada".into(),
            password: Secret::new("sirc1.xxxxxxxx"),
            // Far in the past by the local clock: only `valid_for` counts.
            expires_at: chrono::DateTime::UNIX_EPOCH,
            valid_for,
        })
    }

    fn asked(held: &mut Held, now: Instant) {
        assert_eq!(held.step(now), Step::Ask);
        held.begin(now);
    }

    #[test]
    fn a_login_is_kept_until_a_minute_before_it_lapses_from_when_it_was_asked_for() {
        let now = Instant::now();
        let mut held = Held::default();
        asked(&mut held, now);
        // The answer took half a minute; the login lasts from the request.
        let received = now + Duration::from_secs(30);
        let login = held.take(credential(Duration::from_secs(600)), now, received);
        assert_eq!(login.map(|login| login.account), Ok("ada".into()));
        assert_eq!(held.step(now + Duration::from_secs(300)), Step::Keep);
        assert_eq!(
            held.step(now + Duration::from_secs(541)),
            Step::Ask,
            "renewed a minute before it lapses"
        );
    }

    #[test]
    fn a_failed_renewal_keeps_offering_the_unexpired_login() {
        let now = Instant::now();
        let mut held = Held::default();
        asked(&mut held, now);
        assert!(
            held.take(credential(Duration::from_secs(600)), now, now)
                .is_ok()
        );
        let renew = now + Duration::from_secs(560);
        asked(&mut held, renew);
        let kept = held.take(Err(Error::Network), renew, renew);
        assert_eq!(kept.map(|login| login.account), Ok("ada".into()));
        assert_eq!(held.step(renew + Duration::from_secs(1)), Step::Keep);
        let lapsed = now + Duration::from_secs(601);
        assert!(held.login(lapsed).is_none(), "not once it lapses");
        assert_eq!(held.step(lapsed), Step::Ask);
    }

    #[test]
    fn requests_are_spaced_and_back_off_after_failures() {
        let now = Instant::now();
        let mut held = Held::default();
        asked(&mut held, now);
        let failed = held.take(Err(Error::Network), now, now);
        assert_eq!(failed.err(), Some(Unavailable::Api(Error::Network)));
        assert_eq!(held.step(now + Duration::from_secs(4)), Step::Wait);
        let again = now + Duration::from_secs(10);
        asked(&mut held, again);
        let _ = held.take(Err(Error::UnexpectedStatus(503)), again, again);
        assert_eq!(
            held.step(again + Duration::from_secs(9)),
            Step::Wait,
            "the wait doubles"
        );
        assert_eq!(held.step(again + Duration::from_secs(10)), Step::Ask);
    }

    #[test]
    fn one_request_at_a_time_and_an_expired_answer_is_refused() {
        let now = Instant::now();
        let mut held = Held::default();
        asked(&mut held, now);
        assert_eq!(held.step(now), Step::Wait, "while a request is under way");
        let expired = held.take(credential(Duration::ZERO), now, now);
        assert_eq!(expired.err(), Some(Unavailable::Expired));
        assert!(held.kept.is_none());
        assert_eq!(held.step(now), Step::Wait, "and backs off");
    }

    #[test]
    fn only_a_request_made_says_whether_the_api_was_reached() {
        use super::ServerLogin;
        use crate::initiatives::Endpoint;
        // Nothing listens on the discard port on loopback.
        let client = subspace_api_client::Client::loopback("http://127.0.0.1:9/api", None).unwrap();
        let login = ServerLogin::new(&Endpoint::parse("ircs://irc.example.test:6697").unwrap());
        let (answer, reached) = login.ask(&client);
        assert_eq!(answer.err(), Some(Unavailable::Api(Error::Network)));
        assert_eq!(reached, Some(false), "asked, and the API was not reached");
        let (answer, reached) = login.ask(&client);
        assert_eq!(answer.err(), Some(Unavailable::Waiting));
        assert_eq!(reached, None, "too soon: nothing was asked");
        // A fresh kept login is offered without asking.
        let now = Instant::now();
        login
            .held()
            .take(credential(Duration::from_secs(600)), now, now)
            .unwrap();
        let (answer, reached) = login.ask(&client);
        assert!(answer.is_ok());
        assert_eq!(reached, None);
    }

    #[test]
    fn reload_lets_the_next_request_go_without_the_failure_s_wait() {
        let mut now = Instant::now();
        let mut held = Held::default();
        // Three failures in a row: the next request waits 20 seconds.
        for _ in 0..3 {
            asked(&mut held, now);
            assert!(held.take(Err(Error::Network), now, now).is_err());
            now += Duration::from_secs(30);
        }
        now -= Duration::from_secs(30);
        let past_gap = now + Duration::from_secs(11);
        assert_eq!(held.step(past_gap), Step::Wait, "still the failure's wait");
        held.retry_at = None;
        assert_eq!(held.step(past_gap), Step::Ask, "Reload asks now");
        let soon = now + Duration::from_secs(2);
        assert_eq!(held.step(soon), Step::Wait, "the shortest gap still holds");
    }
}
