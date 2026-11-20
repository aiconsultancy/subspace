use super::reply::lifetime;
use super::{Session, SessionManager, decode, encode};
use crate::Config;
use crate::http::{Reply, Transport, User};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use subspace_account::loopback::Pkce;
use subspace_account::{Account, CredentialStore as _, Error, MemoryStore, Provider as _};
use subspace_core::Secret;

struct Fake {
    calls: AtomicUsize,
    user: Mutex<String>,
    organization: Mutex<Option<String>>,
    bodies: Mutex<Vec<serde_json::Value>>,
}

fn jwt(lifetime: u64) -> String {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + lifetime;
    let payload = URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"));
    format!("e30.{payload}.signature")
}

impl Transport for Fake {
    fn authenticate(&self, _: &str, body: serde_json::Value) -> Result<Reply, Error> {
        let count = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        self.bodies.lock().unwrap().push(body);
        Ok(Reply {
            user: User {
                id: self.user.lock().unwrap().clone(),
                email: "developer@example.com".into(),
                first_name: Some("Example".into()),
                last_name: Some("Developer".into()),
            },
            organization_id: self.organization.lock().unwrap().clone(),
            access_token: Secret::new(jwt(3600)),
            refresh_token: Secret::new(format!("refresh-{count}")),
        })
    }
}

fn fixture() -> (SessionManager, Arc<MemoryStore>, Arc<Fake>) {
    let store = Arc::new(MemoryStore::default());
    let fake = Arc::new(Fake {
        calls: AtomicUsize::default(),
        user: Mutex::new("user_fixture".into()),
        organization: Mutex::new(Some("org_fixture".into())),
        bodies: Mutex::default(),
    });
    let config = Config::new("client_fixture")
        .unwrap()
        .with_resource("example-api")
        .unwrap();
    let mut manager = SessionManager::new(config, store.clone());
    manager.transport = fake.clone();
    (manager, store, fake)
}

fn stored(store: &MemoryStore) -> Session {
    decode(&store.load().unwrap().unwrap()).unwrap()
}

#[test]
fn sign_in_opens_authkit_with_pkce_and_a_loopback_redirect() {
    let (manager, _, _) = fixture();
    let started = manager.start().unwrap();
    let url = url::Url::parse(started.url.as_deref().unwrap()).unwrap();
    assert_eq!(url.path(), "/user_management/authorize");
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], "client_fixture");
    assert_eq!(query["provider"], "authkit");
    assert_eq!(query["code_challenge_method"], "S256");
    assert!(query["redirect_uri"].starts_with("http://127.0.0.1:"));
    assert!(!query["state"].is_empty() && !query["code_challenge"].is_empty());
}

#[test]
fn a_redeemed_code_is_stored_and_tokens_renew_silently() {
    let (manager, store, fake) = fixture();
    let granted = manager.redeem("code-fixture", &Pkce::new()).unwrap();
    let account = manager
        .accept(granted, &subspace_account::Cancel::default(), 0)
        .unwrap();
    assert_eq!(account.name, "Example Developer");
    assert_eq!(account.tenant, "org_fixture");
    assert_eq!(stored(&store).refresh.expose(), "refresh-1");
    let body = fake.bodies.lock().unwrap()[0].clone();
    assert_eq!(body["grant_type"], "authorization_code");
    assert!(body.get("client_secret").is_none());
    let token = manager
        .token_with_validity("example-api", Duration::from_secs(120))
        .unwrap();
    assert_eq!(token.account, account);
    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        1,
        "a valid token is reused"
    );
    let token = manager
        .token_with_validity("example-api", Duration::from_secs(7200))
        .err();
    assert_eq!(token, Some(Error::Service));
    assert_eq!(
        stored(&store).refresh.expose(),
        "refresh-2",
        "rotation is saved"
    );
    assert_eq!(
        fake.bodies.lock().unwrap()[1]["organization_id"],
        "org_fixture"
    );
    assert_eq!(
        manager.token_with_validity("other", Duration::ZERO).err(),
        Some(Error::Unconfigured)
    );
}

#[test]
fn restoration_refreshes_the_stored_session_with_its_original_identity() {
    let (manager, store, fake) = fixture();
    assert_eq!(manager.restore().unwrap(), None);
    let account = Account {
        tenant: "org_fixture".into(),
        id: "user_fixture".into(),
        name: "Example Developer".into(),
        username: "developer@example.com".into(),
    };
    store
        .save(&encode(&Session {
            account: account.clone(),
            refresh: Secret::new("refresh-0"),
        }))
        .unwrap();
    assert_eq!(manager.restore().unwrap().unwrap().id, "user_fixture");
    assert_eq!(fake.bodies.lock().unwrap()[0]["refresh_token"], "refresh-0");
    let (other, store, fake) = fixture();
    store
        .save(&encode(&Session {
            account,
            refresh: Secret::new("refresh-0"),
        }))
        .unwrap();
    *fake.user.lock().unwrap() = "user_other".into();
    assert_eq!(other.restore().err(), Some(Error::InvalidReply));
    *fake.user.lock().unwrap() = "user_fixture".into();
    *fake.organization.lock().unwrap() = Some("org_other".into());
    assert_eq!(other.restore().err(), Some(Error::InvalidReply));
}

#[test]
fn sign_out_forgets_the_session_and_changes_the_generation() {
    let (manager, store, _) = fixture();
    let granted = manager.redeem("code-fixture", &Pkce::new()).unwrap();
    manager
        .accept(granted, &subspace_account::Cancel::default(), 0)
        .unwrap();
    let before = manager.generation();
    manager.sign_out().unwrap();
    assert!(store.load().unwrap().is_none());
    assert_ne!(manager.generation(), before);
    assert_eq!(
        manager
            .token_with_validity("example-api", Duration::ZERO)
            .err(),
        Some(Error::SignInRequired)
    );
}

#[test]
fn an_unreadable_token_expiry_falls_back_to_a_short_lifetime() {
    assert_eq!(lifetime("not-a-jwt"), Duration::from_secs(300));
    let long = lifetime(&jwt(3600));
    assert!(long > Duration::from_secs(3500) && long <= Duration::from_secs(3600));
}

fn signed_in_store(store: &MemoryStore) -> Account {
    let account = Account {
        tenant: "org_fixture".into(),
        id: "user_fixture".into(),
        name: "Example Developer".into(),
        username: "developer@example.com".into(),
    };
    store
        .save(&encode(&Session {
            account: account.clone(),
            refresh: Secret::new("refresh-0"),
        }))
        .unwrap();
    account
}

#[test]
fn committing_a_sign_in_advances_the_generation() {
    let (manager, _, _) = fixture();
    let switching = manager.generation();
    let granted = manager.redeem("code-fixture", &Pkce::new()).unwrap();
    manager
        .accept(granted, &subspace_account::Cancel::default(), switching)
        .unwrap();
    assert_ne!(manager.generation(), switching);
}

#[test]
fn a_refresh_that_goes_stale_keeps_the_rotated_refresh_token() {
    let (manager, store, fake) = fixture();
    signed_in_store(&store);
    let before = manager.generation();
    // An account switch starts while the refresh is in flight.
    manager.invalidate();
    {
        let mut cache = manager.cache.lock().unwrap();
        assert!(manager.load(&mut cache).unwrap());
        assert_eq!(
            manager.renew(&mut cache, before, Duration::ZERO).err(),
            Some(Error::Cancelled)
        );
        assert!(cache.access.is_none(), "the stale access token is dropped");
        assert_eq!(
            cache.session.as_ref().unwrap().refresh.expose(),
            "refresh-1"
        );
    }
    assert_eq!(stored(&store).refresh.expose(), "refresh-1");
    // The switch is cancelled: the retained credential still renews.
    assert_eq!(manager.reconnect().unwrap().unwrap().id, "user_fixture");
    assert_eq!(fake.bodies.lock().unwrap()[1]["refresh_token"], "refresh-1");
    assert_eq!(stored(&store).refresh.expose(), "refresh-2");
}

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
    manager.clock = Arc::new(super::device::VirtualClock::new());
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

#[test]
fn the_browser_flow_is_unchanged_without_device_authorisation() {
    let (manager, _, _) = fixture();
    let started = manager.start().unwrap();
    assert_eq!(started.code, None);
    assert!(started.url.unwrap().contains("/user_management/authorize?"));
}
