//! Who is signed in, as far as IRC goes: the Subspace API's verified user
//! and the username it reports, which is also their IRC account name.
//! Checked once per account; a check that cannot reach the API, or that
//! the API refuses, is tried again with backoff, and only an API that has
//! no username for the person, or no `/me`, settles that no account
//! network can exist.

use std::sync::Arc;
use std::time::{Duration, Instant};

use subspace_api_client::{Client, Error};

use super::env::Env;

/// The first wait before checking again; each further failure doubles it,
/// up to the cap.
const RETRY_FIRST: Duration = Duration::from_secs(5);
/// The cap while the API cannot be reached or answers oddly.
const RETRY_MAX: Duration = Duration::from_secs(300);
/// The cap while the API refuses the account, as when nobody is signed in.
const REFUSED_MAX: Duration = Duration::from_secs(30 * 60);
const RETRY_MAX_DOUBLINGS: u32 = 9;
/// How often the signed-in account is compared with the one checked.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// What the API says of the signed-in person.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Account {
    /// Not checked yet, or the account just changed.
    #[default]
    Unknown,
    /// Signed in, with a username.
    SignedIn(Person),
    /// The API has no username for them: no network comes from the
    /// account.
    Without,
}

/// The signed-in person, as the API verified them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Person {
    /// The API's stable user ID.
    pub id: String,
    /// The username `GET /me` reports, shown before a server confirms it.
    pub username: String,
}

/// What one check found, with the client it used, which stays bound to
/// the account it was made for, and the account generation it began in.
pub(crate) struct Checked {
    pub client: Option<Arc<Client>>,
    pub generation: Option<u64>,
    pub result: Result<Option<Person>, Error>,
}

/// What to do with a check's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Settled(Account),
    /// Check again after a growing wait, up to `cap`.
    Retry {
        cap: Duration,
    },
}

/// Asks the API who is signed in. Blocking.
pub(crate) fn check(env: &dyn Env) -> Checked {
    let generation = env.generation();
    let client = env.client().map(Arc::new);
    let result = match &client {
        Ok(client) => person(client),
        Err(error) => Err(*error),
    };
    Checked {
        client: client.ok(),
        generation,
        result,
    }
}

fn person(client: &Client) -> Result<Option<Person>, Error> {
    let me = client.me()?;
    Ok(me
        .username
        .filter(|name| !name.is_empty())
        .map(|username| Person {
            id: me.user_id,
            username,
        }))
}

/// What a check's `result` means.
pub(crate) fn outcome(result: Result<Option<Person>, Error>) -> Outcome {
    match result {
        Ok(Some(person)) => Outcome::Settled(Account::SignedIn(person)),
        Ok(None) | Err(Error::NotFound) => Outcome::Settled(Account::Without),
        Err(Error::SignInNeeded | Error::Forbidden) => Outcome::Retry { cap: REFUSED_MAX },
        Err(_) => Outcome::Retry { cap: RETRY_MAX },
    }
}

/// When to read the memberships again while they are not known for the
/// signed-in account.
#[derive(Default)]
pub(crate) struct Retry {
    at: Option<Instant>,
    failures: u32,
}

impl Retry {
    /// Reads asked for since the memberships were last known, for tests.
    #[cfg(test)]
    pub fn reads(&self) -> u32 {
        self.failures
    }

    /// Whether to read now, given whether the memberships belong to the
    /// signed-in account (`None` while nobody is). A read is due at once,
    /// then after a growing wait while they still do not.
    pub fn due(&mut self, owned: Option<bool>, now: Instant) -> bool {
        if owned != Some(false) {
            *self = Self::default();
            return false;
        }
        if self.at.is_some_and(|at| now < at) {
            return false;
        }
        let doublings = self.failures.min(RETRY_MAX_DOUBLINGS);
        self.failures = self.failures.saturating_add(1);
        self.at = Some(now + RETRY_FIRST.saturating_mul(1 << doublings).min(RETRY_MAX));
        true
    }
}

/// When to check the account.
#[derive(Default)]
pub(crate) struct Schedule {
    /// A check is running.
    pub running: bool,
    /// Whether a check has settled for the current account.
    settled: bool,
    retry_at: Option<Instant>,
    failures: u32,
    looked_at: Option<Instant>,
}

impl Schedule {
    /// Whether to start a check at `now`.
    pub fn due(&self, now: Instant) -> bool {
        !self.running && !self.settled && self.retry_at.is_none_or(|at| at <= now)
    }

    /// Whether to compare the signed-in account with the checked one at
    /// `now`, which it then records.
    pub fn look(&mut self, now: Instant) -> bool {
        let due = self
            .looked_at
            .is_none_or(|at| now.saturating_duration_since(at) >= LOOK_EVERY);
        if due {
            self.looked_at = Some(now);
        }
        due
    }

    /// Compares the account again at the next look, for tests.
    #[cfg(test)]
    pub fn look_again(&mut self) {
        self.looked_at = None;
    }

    pub fn settle(&mut self) {
        self.running = false;
        self.settled = true;
        self.retry_at = None;
        self.failures = 0;
    }

    /// A check failed at `now`: try again after a growing wait up to `cap`.
    pub fn failed(&mut self, now: Instant, cap: Duration) {
        let doublings = self.failures.min(RETRY_MAX_DOUBLINGS);
        self.failures = self.failures.saturating_add(1);
        self.running = false;
        self.retry_at = Some(now + RETRY_FIRST.saturating_mul(1 << doublings).min(cap));
    }

    /// The account changed: check again now. A check still running is let
    /// finish; its answer is discarded.
    pub fn restart(&mut self) {
        *self = Self {
            running: self.running,
            looked_at: self.looked_at,
            ..Self::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use subspace_api_client::Error;

    use super::{Account, Outcome, Person, REFUSED_MAX, RETRY_MAX, Retry, Schedule, outcome};

    fn ada() -> Person {
        Person {
            id: "user-ada".into(),
            username: "ada".into(),
        }
    }

    #[test]
    fn only_no_username_or_no_me_settles_that_there_is_no_account_network() {
        assert_eq!(
            outcome(Ok(Some(ada()))),
            Outcome::Settled(Account::SignedIn(ada()))
        );
        assert_eq!(outcome(Ok(None)), Outcome::Settled(Account::Without));
        assert_eq!(
            outcome(Err(Error::NotFound)),
            Outcome::Settled(Account::Without)
        );
        for refused in [Error::SignInNeeded, Error::Forbidden] {
            assert_eq!(
                outcome(Err(refused)),
                Outcome::Retry { cap: REFUSED_MAX },
                "{refused:?} is checked again"
            );
        }
        for failed in [
            Error::Network,
            Error::UnexpectedStatus(502),
            Error::InvalidResponse,
        ] {
            assert_eq!(outcome(Err(failed)), Outcome::Retry { cap: RETRY_MAX });
        }
    }

    #[test]
    fn failed_checks_back_off_to_their_cap_and_an_account_change_checks_at_once() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        assert!(schedule.due(now));
        schedule.running = true;
        assert!(!schedule.due(now), "one check at a time");
        schedule.failed(now, RETRY_MAX);
        assert!(!schedule.due(now + Duration::from_secs(4)));
        assert!(schedule.due(now + Duration::from_secs(5)));
        for _ in 0..12 {
            schedule.failed(now, RETRY_MAX);
        }
        assert!(schedule.due(now + RETRY_MAX), "capped");
        for _ in 0..12 {
            schedule.failed(now, REFUSED_MAX);
        }
        assert!(!schedule.due(now + RETRY_MAX), "a refusal waits longer");
        assert!(schedule.due(now + REFUSED_MAX));
        schedule.settle();
        assert!(!schedule.due(now + REFUSED_MAX * 2), "settled");
        schedule.restart();
        assert!(schedule.due(now));
    }

    #[test]
    fn memberships_not_known_for_the_account_are_read_again_with_backoff() {
        let now = Instant::now();
        let mut retry = Retry::default();
        assert!(!retry.due(None, now), "nobody signed in");
        assert!(!retry.due(Some(true), now), "known for the account");
        assert!(retry.due(Some(false), now), "read at once");
        assert!(!retry.due(Some(false), now + Duration::from_secs(4)));
        assert!(retry.due(Some(false), now + Duration::from_secs(5)));
        assert!(!retry.due(Some(false), now + Duration::from_secs(14)));
        assert!(retry.due(Some(false), now + Duration::from_secs(15)));
        assert!(!retry.due(Some(true), now + Duration::from_secs(16)));
        assert!(
            retry.due(Some(false), now + Duration::from_secs(16)),
            "known again resets the wait"
        );
    }

    #[test]
    fn the_account_is_compared_at_most_once_a_second() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        assert!(schedule.look(now));
        assert!(!schedule.look(now + Duration::from_millis(500)));
        assert!(schedule.look(now + Duration::from_secs(1)));
    }
}
