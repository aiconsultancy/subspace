//! One server's issued login: asked for with that server's host and port,
//! kept in memory until shortly before it expires, and never asked for
//! again sooner than a minimum interval, or a growing wait after a failure.
//! A login is only ever offered to the server it was issued for. Its
//! lifetime runs from when it was received, by the API's clock, so a fast
//! or slow local clock changes nothing.

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
        let now = Instant::now();
        {
            let mut held = self.held();
            match held.step(now) {
                Step::Keep => return held.login(now).ok_or(Unavailable::Waiting),
                Step::Wait => return Err(Unavailable::Waiting),
                Step::Ask => held.begin(now),
            }
        }
        let answer = client.irc_credential(&self.host, self.port);
        self.held().take(answer, now, Instant::now())
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
                return Ok(self.keep(credential, received));
            }
            Ok(_) => Unavailable::Expired,
            Err(error) => Unavailable::Api(error),
        };
        self.fail(asked);
        self.login(received).ok_or(unavailable)
    }

    fn keep(&mut self, credential: IrcCredential, received: Instant) -> Login {
        self.failures = 0;
        self.retry_at = None;
        let login = Login {
            account: credential.account,
            password: credential.password,
            valid_until: received + credential.valid_for,
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
    fn a_login_is_kept_until_a_minute_before_it_lapses_whatever_the_local_clock() {
        let now = Instant::now();
        let mut held = Held::default();
        asked(&mut held, now);
        let login = held.take(credential(Duration::from_secs(600)), now, now);
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
}
