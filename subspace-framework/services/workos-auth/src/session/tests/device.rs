use super::super::{SessionManager, device::VirtualClock};
use super::{jwt, stored};
use crate::Config;
use crate::http::{Reply, Transport, User};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use subspace_account::{Account, CredentialStore as _, Error, MemoryStore, Provider as _};
use subspace_core::Secret;

/// A WorkOS device authority answering polls and form refreshes from
/// scripts, and recording every form it was sent.
struct DeviceFake {
    polls: Mutex<Vec<crate::http::DevicePoll>>,
    forms: Mutex<Vec<Vec<(String, String)>>>,
    refreshes: Mutex<Vec<Reply>>,
    refresh_forms: Mutex<Vec<Vec<(String, String)>>>,
    complete: bool,
}

fn owned(form: &[(&str, &str)]) -> Vec<(String, String)> {
    form.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

impl Transport for DeviceFake {
    fn authenticate(&self, _: &str, _: serde_json::Value) -> Result<Reply, Error> {
        Err(Error::Service)
    }

    fn authorize_device(
        &self,
        url: &str,
        client_id: &str,
    ) -> Result<crate::http::DeviceStart, Error> {
        assert!(url.ends_with("/user_management/authorize/device"));
        assert_eq!(client_id, "client_fixture");
        Ok(crate::http::DeviceStart {
            device_code: Secret::new("device-fixture"),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://auth.example.com/device".into(),
            verification_uri_complete: self
                .complete
                .then(|| "https://auth.example.com/device?user_code=ABCD-EFGH".into()),
            expires_in: 300,
            interval: Some(1),
        })
    }

    fn poll_device(
        &self,
        url: &str,
        form: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<crate::http::DevicePoll, Error> {
        assert!(url.ends_with("/user_management/authenticate"));
        assert!(timeout > Duration::ZERO && timeout <= crate::http::REQUEST_TIMEOUT);
        self.forms.lock().unwrap().push(owned(form));
        let mut polls = self.polls.lock().unwrap();
        if polls.is_empty() {
            return Err(Error::NotPermitted);
        }
        Ok(polls.remove(0))
    }

    fn refresh_form(&self, url: &str, form: &[(&str, &str)]) -> Result<Reply, Error> {
        assert!(url.ends_with("/user_management/authenticate"));
        self.refresh_forms.lock().unwrap().push(owned(form));
        let mut refreshes = self.refreshes.lock().unwrap();
        if refreshes.is_empty() {
            return Err(Error::SignInRequired);
        }
        Ok(refreshes.remove(0))
    }
}

/// WorkOS's tokens for `user`, in `organisation` when it pins one.
fn reply_for(user: &str, organisation: Option<&str>, refresh: &str, lifetime: u64) -> Reply {
    Reply {
        user: User {
            id: user.into(),
            email: format!("{user}@example.com"),
            first_name: Some("Example".into()),
            last_name: Some(user.into()),
        },
        organization_id: organisation.map(str::to_owned),
        access_token: Secret::new(jwt(lifetime)),
        refresh_token: Secret::new(refresh),
    }
}

fn granted_reply() -> crate::http::DevicePoll {
    crate::http::DevicePoll::Granted(Box::new(Reply {
        user: User {
            id: "user_fixture".into(),
            email: "developer@example.com".into(),
            first_name: Some("Example".into()),
            last_name: Some("Developer".into()),
        },
        organization_id: None,
        access_token: Secret::new(jwt(3600)),
        refresh_token: Secret::new("refresh-device"),
    }))
}

fn granted(reply: Reply) -> crate::http::DevicePoll {
    crate::http::DevicePoll::Granted(Box::new(reply))
}

fn device_fixture(
    polls: Vec<crate::http::DevicePoll>,
) -> (SessionManager, Arc<MemoryStore>, Arc<DeviceFake>) {
    let store = Arc::new(MemoryStore::default());
    let fake = Arc::new(DeviceFake {
        polls: Mutex::new(polls),
        forms: Mutex::default(),
        refreshes: Mutex::default(),
        refresh_forms: Mutex::default(),
        complete: true,
    });
    let config = Config::new("client_fixture")
        .unwrap()
        .with_resource("example-api")
        .unwrap()
        .with_device_authorization();
    let mut manager = SessionManager::new(config, store.clone());
    manager.transport = fake.clone();
    manager.clock = Arc::new(VirtualClock::new());
    (manager, store, fake)
}

/// Signs in through the device flow, answering the first poll with `reply`.
fn device_sign_in(manager: &SessionManager, fake: &DeviceFake, reply: Reply) -> Account {
    fake.polls.lock().unwrap().push(granted(reply));
    let started = manager.start().unwrap();
    let generation = manager.generation();
    manager
        .finish(started, &subspace_account::Cancel::default(), generation)
        .unwrap()
}

#[test]
fn device_sessions_renew_with_the_form_encoded_refresh_grant() {
    let (manager, store, fake) = device_fixture(Vec::new());
    let account = device_sign_in(
        &manager,
        &fake,
        reply_for("user_fixture", None, "refresh-a", 30),
    );
    assert_eq!(account.tenant, "");
    fake.refreshes
        .lock()
        .unwrap()
        .push(reply_for("user_fixture", None, "refresh-b", 3600));
    // The short-lived first token forces a renewal.
    let token = manager
        .token_with_validity("example-api", Duration::from_secs(120))
        .unwrap();
    assert_eq!(token.account, account);
    let forms = fake.refresh_forms.lock().unwrap();
    assert_eq!(
        forms.as_slice(),
        [vec![
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("client_id".to_owned(), "client_fixture".to_owned()),
            ("refresh_token".to_owned(), "refresh-a".to_owned()),
        ]],
        "exactly the refresh form device clients send"
    );
    assert_eq!(stored(&store).refresh.expose(), "refresh-b");
}

#[test]
fn an_organisation_in_the_device_reply_is_pinned_for_renewal() {
    let (manager, store, fake) = device_fixture(Vec::new());
    let account = device_sign_in(
        &manager,
        &fake,
        reply_for("user_fixture", Some("org_fixture"), "refresh-a", 30),
    );
    assert_eq!(account.tenant, "org_fixture");
    fake.refreshes.lock().unwrap().push(reply_for(
        "user_fixture",
        Some("org_fixture"),
        "refresh-b",
        30,
    ));
    assert!(
        manager
            .token_with_validity("example-api", Duration::from_secs(120))
            .is_err(),
        "the renewed token is still too short for two minutes"
    );
    assert!(
        fake.refresh_forms.lock().unwrap()[0]
            .contains(&("organization_id".into(), "org_fixture".into()))
    );
    // A renewal that lands in another organisation is refused, and the
    // stored session keeps its organisation.
    fake.refreshes.lock().unwrap().push(reply_for(
        "user_fixture",
        Some("org_other"),
        "refresh-c",
        3600,
    ));
    assert_eq!(
        manager
            .token_with_validity("example-api", Duration::from_secs(120))
            .err(),
        Some(Error::InvalidReply)
    );
    assert_eq!(stored(&store).account.tenant, "org_fixture");
}

#[test]
fn switching_account_replaces_the_one_active_device_session() {
    let (manager, store, fake) = device_fixture(Vec::new());
    let first = device_sign_in(
        &manager,
        &fake,
        reply_for("user_ada", None, "refresh-ada", 3600),
    );
    let lease = manager.generation();
    let token = manager
        .token_with_validity("example-api", Duration::from_secs(60))
        .unwrap();
    assert_eq!(token.account, first);
    let second = device_sign_in(
        &manager,
        &fake,
        reply_for("user_grace", None, "refresh-grace", 3600),
    );
    assert_ne!(second.id, first.id);
    assert_ne!(
        manager.generation(),
        lease,
        "leases of the first account end"
    );
    assert_eq!(stored(&store).account.id, "user_grace");
    assert_eq!(stored(&store).refresh.expose(), "refresh-grace");
    let token = manager
        .token_with_validity("example-api", Duration::from_secs(60))
        .unwrap();
    assert_eq!(token.account, second);
}

#[test]
fn device_sign_in_shows_the_code_polls_and_stores_the_session() {
    use crate::http::DevicePoll;
    let (manager, store, fake) = device_fixture(vec![
        DevicePoll::Pending,
        DevicePoll::SlowDown,
        DevicePoll::Pending,
        granted_reply(),
    ]);
    let started = manager.start().unwrap();
    assert_eq!(
        started.url.as_deref(),
        Some("https://auth.example.com/device?user_code=ABCD-EFGH")
    );
    assert_eq!(started.code.as_deref(), Some("ABCD-EFGH"));
    let generation = manager.generation();
    let account = manager
        .finish(started, &subspace_account::Cancel::default(), generation)
        .unwrap();
    assert_eq!(account.id, "user_fixture");
    assert_eq!(account.tenant, "", "no WorkOS organisation is pinned");
    assert_eq!(stored(&store).refresh.expose(), "refresh-device");
    assert_ne!(manager.generation(), generation);
    let forms = fake.forms.lock().unwrap();
    assert_eq!(forms.len(), 4);
    assert!(forms[0].contains(&(
        "grant_type".into(),
        "urn:ietf:params:oauth:grant-type:device_code".into()
    )));
    assert!(forms[0].contains(&("device_code".into(), "device-fixture".into())));
    assert!(forms[0].contains(&("client_id".into(), "client_fixture".into())));
    assert!(
        manager
            .token_with_validity("example-api", Duration::from_secs(60))
            .is_ok()
    );
}

#[test]
fn device_sign_in_stops_on_denial_or_cancellation() {
    let (manager, store, _) = device_fixture(vec![]);
    let started = manager.start().unwrap();
    let generation = manager.generation();
    assert_eq!(
        manager
            .finish(started, &subspace_account::Cancel::default(), generation)
            .err(),
        Some(Error::NotPermitted)
    );
    assert!(store.load().unwrap().is_none());

    let (manager, store, _) = device_fixture(vec![crate::http::DevicePoll::Pending]);
    let started = manager.start().unwrap();
    let cancel = subspace_account::Cancel::default();
    assert!(cancel.cancel());
    assert_eq!(
        manager.finish(started, &cancel, manager.generation()).err(),
        Some(Error::Cancelled)
    );
    assert!(store.load().unwrap().is_none());
}
