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
