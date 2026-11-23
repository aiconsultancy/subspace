//! One server's issued login: asked for with that server's host and port,
//! kept in memory until shortly before it expires, and never asked for
//! again sooner than a minimum interval, or a growing wait after a failure.
//! A login is only ever offered to the server it was issued for.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeDelta, Utc};
use subspace_api_client::{Client, Error, IrcCredential};
use subspace_framework::core::Secret;

use crate::initiatives::Endpoint;

/// A kept login is asked for again this long before it expires.
const RENEW_BEFORE: TimeDelta = TimeDelta::seconds(60);
/// The least time between two requests for one server.
const MIN_INTERVAL: Duration = Duration::from_secs(10);
/// The first wait after a failed request; each further failure doubles it.
const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);
const RETRY_MAX_DOUBLINGS: u32 = 6;

/// A login for one connect to the server.
pub(crate) struct Login {
    pub account: String,
    pub password: Secret,
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
    kept: Option<Kept>,
    asked_at: Option<Instant>,
    failures: u32,
    retry_at: Option<Instant>,
}

struct Kept {
    account: String,
    password: Secret,
    expires_at: DateTime<Utc>,
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
    /// new one when the API may be asked. Blocking; one request at a time
    /// per server.
    pub(crate) fn login(&self, client: &Client) -> Result<Login, Unavailable> {
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (now, clock) = (Instant::now(), Utc::now());
        match held.step(now, clock) {
            Step::Keep => held.login().ok_or(Unavailable::Waiting),
            Step::Wait => Err(Unavailable::Waiting),
            Step::Ask => {
                let answer = client.irc_credential(&self.host, self.port);
                held.take(answer, now, Utc::now())
            }
        }
    }

    /// The server refused the kept login: drop it, so the next connect
    /// asks again once that is allowed.
    pub(crate) fn forget(&self) {
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        held.kept = None;
        held.fail(Instant::now());
    }
}

impl Held {
    fn step(&self, now: Instant, clock: DateTime<Utc>) -> Step {
        if self
            .kept
            .as_ref()
            .is_some_and(|kept| clock < kept.expires_at - RENEW_BEFORE)
        {
            return Step::Keep;
        }
        let too_soon = self.retry_at.is_some_and(|at| now < at)
            || self
                .asked_at
                .is_some_and(|at| now.saturating_duration_since(at) < MIN_INTERVAL);
        match (too_soon, self.kept.as_ref()) {
            (false, _) => Step::Ask,
            // Too soon to renew, but the kept login has not expired yet.
            (true, Some(kept)) if clock < kept.expires_at => Step::Keep,
            (true, _) => Step::Wait,
        }
    }

    fn login(&self) -> Option<Login> {
        self.kept.as_ref().map(|kept| Login {
            account: kept.account.clone(),
            password: kept.password.clone(),
        })
    }

    /// Takes the API's `answer` to a request made at `now`.
    fn take(
        &mut self,
        answer: Result<IrcCredential, Error>,
        now: Instant,
        clock: DateTime<Utc>,
    ) -> Result<Login, Unavailable> {
        self.asked_at = Some(now);
        let credential = match answer {
            Ok(credential) if credential.expires_at > clock => credential,
            Ok(_) => {
                self.fail(now);
                return Err(Unavailable::Expired);
            }
            Err(error) => {
                self.fail(now);
                return Err(Unavailable::Api(error));
            }
        };
        self.failures = 0;
        self.retry_at = None;
        let login = Login {
            account: credential.account.clone(),
            password: credential.password.clone(),
        };
        self.kept = Some(Kept {
            account: credential.account,
            password: credential.password,
            expires_at: credential.expires_at,
        });
        Ok(login)
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

    use chrono::{TimeDelta, Utc};
    use subspace_api_client::{Error, IrcCredential};
    use subspace_framework::core::Secret;

    use super::{Held, Step, Unavailable};

    fn credential(valid_for: TimeDelta) -> Result<IrcCredential, Error> {
        Ok(test_credential(Utc::now() + valid_for))
    }

    fn test_credential(expires_at: chrono::DateTime<Utc>) -> IrcCredential {
        subspace_api_client::IrcCredential {
            account: "ada".into(),
            password: Secret::new("sirc1.xxxxxxxx"),
            expires_at,
        }
    }

    #[test]
    fn a_fresh_login_is_kept_until_shortly_before_it_expires() {
        let (now, clock) = (Instant::now(), Utc::now());
        let mut held = Held::default();
        assert_eq!(held.step(now, clock), Step::Ask);
        let login = held.take(credential(TimeDelta::minutes(10)), now, clock);
        assert_eq!(login.map(|login| login.account), Ok("ada".into()));
        let later = clock + TimeDelta::minutes(5);
        assert_eq!(held.step(now + Duration::from_secs(300), later), Step::Keep);
        let renew = clock + TimeDelta::seconds(9 * 60 + 30);
        assert_eq!(
            held.step(now + Duration::from_secs(570), renew),
            Step::Ask,
            "renewed a minute before it expires"
        );
    }

    #[test]
    fn requests_are_spaced_and_back_off_after_failures() {
        let (now, clock) = (Instant::now(), Utc::now());
        let mut held = Held::default();
        let failed = held.take(Err(Error::Network), now, clock);
        assert_eq!(failed.err(), Some(Unavailable::Api(Error::Network)));
        assert_eq!(held.step(now + Duration::from_secs(4), clock), Step::Wait);
        assert_eq!(held.step(now + Duration::from_secs(10), clock), Step::Ask);
        let again = now + Duration::from_secs(10);
        let _ = held.take(Err(Error::UnexpectedStatus(503)), again, clock);
        assert_eq!(
            held.step(again + Duration::from_secs(9), clock),
            Step::Wait,
            "the wait doubles"
        );
        assert_eq!(held.step(again + Duration::from_secs(10), clock), Step::Ask);
    }

    #[test]
    fn a_login_that_arrives_expired_is_refused() {
        let (now, clock) = (Instant::now(), Utc::now());
        let mut held = Held::default();
        let expired = held.take(
            Ok(test_credential(clock - TimeDelta::seconds(1))),
            now,
            clock,
        );
        assert_eq!(expired.err(), Some(Unavailable::Expired));
        assert!(held.kept.is_none());
        assert_eq!(held.step(now, clock), Step::Wait, "and backs off");
    }

    #[test]
    fn a_short_lived_login_is_used_once_but_not_renewed_too_soon() {
        let (now, clock) = (Instant::now(), Utc::now());
        let mut held = Held::default();
        assert!(
            held.take(credential(TimeDelta::seconds(30)), now, clock)
                .is_ok()
        );
        assert_eq!(
            held.step(now + Duration::from_secs(2), clock),
            Step::Keep,
            "within the minimum interval the unexpired login is offered"
        );
        assert_eq!(held.step(now + Duration::from_secs(11), clock), Step::Ask);
    }
}
