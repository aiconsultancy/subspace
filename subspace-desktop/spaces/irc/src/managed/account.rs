//! Who is signed in, as far as IRC goes: whether the Subspace API knows a
//! username for them and issues IRC sign-ins. Checked once per account,
//! and again with backoff after a check that could not reach the API.

use std::sync::Arc;
use std::time::{Duration, Instant};

use subspace_api_client::{Client, Error};

/// The first wait before checking again after the API could not be
/// reached; each further failure doubles it, up to the maximum.
const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);
const RETRY_MAX_DOUBLINGS: u32 = 6;
/// How often the signed-in account is compared with the one checked.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// What the API says of the signed-in person and IRC.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Account {
    /// Not checked yet, or the account just changed.
    #[default]
    Unknown,
    /// Signed in, with a username, and the API issues IRC sign-ins.
    SignedIn(SignedIn),
    /// Signed out, or the API has no username for them or issues no IRC
    /// sign-ins: no network comes from the account.
    Without,
}

/// A person the API issues IRC sign-ins for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignedIn {
    /// The username `GET /me` reports, shown before connecting.
    pub username: String,
    /// The IRC account name the credential route answered, which is the
    /// nick. The one issued at each connect is the one used.
    pub account: String,
}

/// What one check found, with the client it used, which stays bound to
/// the account it was made for.
pub(crate) struct Checked {
    pub client: Option<Arc<Client>>,
    pub result: Result<Option<SignedIn>, Error>,
}

/// Asks the API who is signed in and whether it issues IRC sign-ins. The
/// credential asked for here is dropped at once. Blocking.
pub(crate) fn check() -> Checked {
    match Client::new() {
        Ok(client) => {
            let client = Arc::new(client);
            Checked {
                result: signed_in(&client),
                client: Some(client),
            }
        }
        Err(error) => Checked {
            client: None,
            result: Err(error),
        },
    }
}

fn signed_in(client: &Client) -> Result<Option<SignedIn>, Error> {
    let Some(username) = client.me()?.username.filter(|name| !name.is_empty()) else {
        return Ok(None);
    };
    match client.irc_credential() {
        Ok(credential) => Ok(Some(SignedIn {
            username,
            account: credential.account,
        })),
        Err(Error::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Whether the account changed while `checked` ran, so its answer is for
/// an account no longer signed in.
pub(crate) fn stale(checked: &Checked) -> bool {
    checked
        .client
        .as_ref()
        .is_some_and(|client| client.account_changed())
}

/// The account a check's `result` shows, or `None` when the API could not
/// be reached or answered oddly, so it is checked again later.
pub(crate) fn outcome(result: Result<Option<SignedIn>, Error>) -> Option<Account> {
    match result {
        Ok(Some(signed_in)) => Some(Account::SignedIn(signed_in)),
        Ok(None) => Some(Account::Without),
        Err(Error::Network | Error::UnexpectedStatus(_) | Error::InvalidResponse) => None,
        Err(_) => Some(Account::Without),
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

    pub fn settle(&mut self) {
        self.running = false;
        self.settled = true;
        self.retry_at = None;
        self.failures = 0;
    }

    /// A check failed at `now`: try again after a growing wait.
    pub fn failed(&mut self, now: Instant) {
        let doublings = self.failures.min(RETRY_MAX_DOUBLINGS);
        self.failures = self.failures.saturating_add(1);
        self.running = false;
        self.retry_at = Some(now + RETRY_FIRST.saturating_mul(1 << doublings).min(RETRY_MAX));
    }

    /// The account changed: check again now.
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

    use super::{Account, Schedule, SignedIn, outcome};

    fn ada() -> SignedIn {
        SignedIn {
            username: "ada".into(),
            account: "ada".into(),
        }
    }

    #[test]
    fn a_check_settles_the_account_unless_the_api_was_unreachable() {
        assert_eq!(outcome(Ok(Some(ada()))), Some(Account::SignedIn(ada())));
        assert_eq!(
            outcome(Ok(None)),
            Some(Account::Without),
            "no username or route"
        );
        for refused in [Error::SignInNeeded, Error::Forbidden, Error::NotFound] {
            assert_eq!(outcome(Err(refused)), Some(Account::Without), "{refused:?}");
        }
        for failed in [
            Error::Network,
            Error::UnexpectedStatus(502),
            Error::InvalidResponse,
        ] {
            assert_eq!(outcome(Err(failed)), None, "{failed:?} is checked again");
        }
    }

    #[test]
    fn failed_checks_back_off_and_an_account_change_checks_at_once() {
        let now = Instant::now();
        let mut schedule = Schedule::default();
        assert!(schedule.due(now));
        schedule.running = true;
        assert!(!schedule.due(now), "one check at a time");
        schedule.failed(now);
        assert!(!schedule.due(now + Duration::from_secs(4)));
        assert!(schedule.due(now + Duration::from_secs(5)));
        schedule.failed(now);
        assert!(!schedule.due(now + Duration::from_secs(9)));
        for _ in 0..10 {
            schedule.failed(now);
        }
        assert!(schedule.due(now + Duration::from_secs(300)), "capped");
        schedule.settle();
        assert!(!schedule.due(now + Duration::from_secs(900)), "settled");
        schedule.restart();
        assert!(schedule.due(now));
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
