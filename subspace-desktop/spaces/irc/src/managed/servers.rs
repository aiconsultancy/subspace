//! The account's servers: for each IRC server of its fleets, whether the
//! API vouches for it, learnt by asking for that server's login (the
//! probe), and that server's own login. A server the API does not vouch
//! for (404) gets no network; others are unaffected. A probe that fails
//! otherwise is tried again with backoff.

use std::sync::Arc;
use std::time::{Duration, Instant};

use subspace_api_client::Error;

use super::login::{ServerLogin, Unavailable};
use super::plan::{Server, same_endpoint};
use crate::initiatives::Endpoint;

const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);
const REFUSED_MAX: Duration = Duration::from_secs(30 * 60);
const RETRY_MAX_DOUBLINGS: u32 = 9;

/// One server and what is known of it.
pub(crate) struct Entry {
    pub server: Server,
    pub login: Arc<ServerLogin>,
    probe: Probe,
    /// Probes in a row that failed.
    failures: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Probe {
    /// To ask, once the time has come (`None`: now).
    Due(Option<Instant>),
    Asking,
    /// The API vouches for it and issued a login for this account name.
    Vouched(String),
    /// The API does not vouch for it.
    Refused,
}

/// The account's servers, in the order the fleets name them.
#[derive(Default)]
pub(crate) struct Servers {
    entries: Vec<Entry>,
}

impl Servers {
    /// Follows the fleets' `servers`: keeps what is known of each server
    /// still listed, adds the new ones and drops the rest with their
    /// logins.
    pub fn follow(&mut self, servers: Vec<Server>) {
        let mut kept: Vec<Entry> = Vec::with_capacity(servers.len());
        for server in servers {
            let known = self
                .entries
                .iter()
                .position(|entry| same_endpoint(&entry.server.endpoint, &server.endpoint));
            kept.push(match known {
                Some(index) => {
                    let mut entry = self.entries.swap_remove(index);
                    entry.server = server;
                    entry
                }
                None => Entry {
                    login: Arc::new(ServerLogin::new(&server.endpoint)),
                    server,
                    probe: Probe::Due(None),
                    failures: 0,
                },
            });
        }
        self.entries = kept;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Makes every probe waiting after a failure due now, and lets each
    /// server's login be asked for again, as Reload asks.
    pub fn retry_now(&mut self) {
        for entry in &mut self.entries {
            if matches!(entry.probe, Probe::Due(Some(_))) {
                entry.probe = Probe::Due(None);
            }
            entry.login.retry_now();
        }
    }

    /// The servers whose probe is due at `now`, now marked as asking.
    pub fn due(&mut self, now: Instant) -> Vec<(Endpoint, Arc<ServerLogin>)> {
        self.entries
            .iter_mut()
            .filter(|entry| matches!(entry.probe, Probe::Due(at) if at.is_none_or(|at| at <= now)))
            .map(|entry| {
                entry.probe = Probe::Asking;
                (entry.server.endpoint.clone(), Arc::clone(&entry.login))
            })
            .collect()
    }

    /// Takes the answer to the probe of `endpoint` made before `now`: the
    /// account name the server's login was issued for, or why there is
    /// none. True when the servers the API vouches for changed.
    pub fn answer(
        &mut self,
        endpoint: &Endpoint,
        answer: Result<String, Unavailable>,
        now: Instant,
    ) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| same_endpoint(&entry.server.endpoint, endpoint))
            .filter(|entry| entry.probe == Probe::Asking)
        else {
            return false;
        };
        entry.probe = probe(answer, entry.failures, now);
        entry.failures = match entry.probe {
            Probe::Due(_) => entry.failures.saturating_add(1),
            _ => 0,
        };
        matches!(entry.probe, Probe::Vouched(_))
    }

    /// The servers the API vouches for, with the account name it issued.
    pub fn vouched(&self) -> impl Iterator<Item = (&Entry, &str)> {
        self.entries.iter().filter_map(|entry| match &entry.probe {
            Probe::Vouched(account) => Some((entry, account.as_str())),
            _ => None,
        })
    }
}

/// What a probe's `answer` at `now`, after `failures` in a row, leaves the
/// server as.
fn probe(answer: Result<String, Unavailable>, failures: u32, now: Instant) -> Probe {
    let doublings = failures.min(RETRY_MAX_DOUBLINGS);
    let retry = |cap: Duration| {
        Probe::Due(Some(
            now + RETRY_FIRST.saturating_mul(1 << doublings).min(cap),
        ))
    };
    match answer {
        Ok(account) => Probe::Vouched(account),
        Err(Unavailable::Api(Error::NotFound)) => Probe::Refused,
        Err(Unavailable::Api(Error::SignInNeeded | Error::Forbidden)) => retry(REFUSED_MAX),
        Err(_) => retry(RETRY_MAX),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use subspace_api_client::Error;

    use super::{Servers, Unavailable};
    use crate::initiatives::Endpoint;
    use crate::managed::plan::Server;

    fn server(host: &str) -> Server {
        Server {
            endpoint: Endpoint {
                host: host.into(),
                port: 6697,
                tls: true,
            },
            fleets: vec![host.into()],
        }
    }

    fn vouched(servers: &Servers) -> Vec<(String, String)> {
        servers
            .vouched()
            .map(|(entry, account)| (entry.server.endpoint.host.clone(), account.to_owned()))
            .collect()
    }

    #[test]
    fn each_server_is_vouched_for_or_refused_on_its_own() {
        let now = Instant::now();
        let mut servers = Servers::default();
        servers.follow(vec![server("irc.example.com"), server("irc.example.org")]);
        let due = servers.due(now);
        assert_eq!(due.len(), 2);
        assert!(servers.due(now).is_empty(), "asked once");
        assert!(servers.answer(&due[0].0, Ok("ada".into()), now));
        assert!(!servers.answer(&due[1].0, Err(Unavailable::Api(Error::NotFound)), now));
        assert_eq!(
            vouched(&servers),
            [("irc.example.com".into(), "ada".into())]
        );
        assert!(
            servers.due(now + Duration::from_secs(3600)).is_empty(),
            "a refused server is not asked again"
        );
    }

    #[test]
    fn a_failed_probe_is_asked_again_later() {
        let now = Instant::now();
        let mut servers = Servers::default();
        servers.follow(vec![server("irc.example.com")]);
        let due = servers.due(now);
        servers.answer(&due[0].0, Err(Unavailable::Api(Error::Network)), now);
        assert!(servers.due(now + Duration::from_secs(4)).is_empty());
        assert_eq!(servers.due(now + Duration::from_secs(5)).len(), 1);
        let again = now + Duration::from_secs(5);
        servers.answer(&due[0].0, Err(Unavailable::Api(Error::Forbidden)), again);
        assert!(
            servers.due(again + Duration::from_secs(9)).is_empty(),
            "the wait doubles"
        );
        assert_eq!(
            servers.due(again + Duration::from_secs(10)).len(),
            1,
            "refusals are asked again too"
        );
    }

    #[test]
    fn following_the_fleets_keeps_known_servers_and_drops_gone_ones() {
        let now = Instant::now();
        let mut servers = Servers::default();
        servers.follow(vec![server("irc.example.com")]);
        let due = servers.due(now);
        servers.answer(&due[0].0, Ok("ada".into()), now);
        servers.follow(vec![server("irc.example.org"), server("irc.example.com")]);
        assert_eq!(vouched(&servers).len(), 1, "known answers stay");
        assert_eq!(servers.due(now).len(), 1, "only the new server is asked");
        servers.follow(vec![server("irc.example.org")]);
        assert!(
            vouched(&servers).is_empty(),
            "a gone server goes with its login"
        );
        assert!(
            !servers.answer(&due[0].0, Ok("ada".into()), now),
            "a late answer for a gone server changes nothing"
        );
    }
}
