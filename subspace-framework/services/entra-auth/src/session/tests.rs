use super::encode;
use crate::oidc::{Granted, Provider, Session};
use crate::{Account, Config, CredentialStore, Error, SessionManager};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use subspace_core::Secret;

#[derive(Default)]
struct Memory(Mutex<Option<Secret>>, AtomicBool, AtomicBool);
impl CredentialStore for Memory {
    fn load(&self) -> Result<Option<Secret>, Error> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn save(&self, value: &Secret) -> Result<(), Error> {
        if self.1.load(Ordering::SeqCst) {
            return Err(Error::Keychain);
        }
        *self.0.lock().unwrap() = Some(value.clone());
        Ok(())
    }
    fn delete(&self) -> Result<(), Error> {
        if self.2.load(Ordering::SeqCst) {
            return Err(Error::Keychain);
        }
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
struct Fake {
    count: AtomicUsize,
    failure: Option<Error>,
    scopes: Mutex<Vec<String>>,
    refreshes: Mutex<Vec<String>>,
}
impl Provider for Fake {
    fn refresh(&self, session: &Session, scope: Option<&str>) -> Result<Granted, Error> {
        self.scopes
            .lock()
            .unwrap()
            .push(scope.unwrap_or_default().into());
        self.refreshes
            .lock()
            .unwrap()
            .push(session.refresh.expose().to_owned());
        self.count.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.failure {
            return Err(error);
        }
        let mut next = session.clone();
        next.refresh = Secret::new("rotated-fixture");
        Ok(Granted {
            session: next,
            access: Secret::new(scope.unwrap_or("access-fixture")),
            expires_at: std::time::Instant::now() + Duration::from_secs(3600),
        })
    }
}
fn fixture(failure: Option<Error>) -> (Arc<SessionManager>, Arc<Memory>, Arc<Fake>) {
    let tenant = "00000000-0000-0000-0000-000000000001";
    let store = Arc::new(Memory::default());
    let account = Account {
        tenant: tenant.into(),
        id: "00000000-0000-0000-0000-000000000003".into(),
        name: "Example Developer".into(),
        username: "developer@example.test".into(),
    };
    store
        .save(&encode(&Session {
            account,
            subject: "subject".into(),
            refresh: Secret::new("refresh-fixture"),
            nonce: Secret::new("nonce-fixture"),
        }))
        .unwrap();
    let provider = Arc::new(Fake {
        count: AtomicUsize::default(),
        failure,
        scopes: Mutex::default(),
        refreshes: Mutex::default(),
    });
    let mut manager = SessionManager::new(
        Config::new(tenant, "00000000-0000-0000-0000-000000000002")
            .unwrap()
            .with_resource(
                "example-api",
                "00000000-0000-0000-0000-000000000004",
                "API.Access",
            )
            .unwrap()
            .with_resource(
                "issues",
                "00000000-0000-0000-0000-000000000005",
                "API.Access",
            )
            .unwrap(),
        store.clone(),
    );
    manager.provider = provider.clone();
    (Arc::new(manager), store, provider)
}
#[test]
fn restoration_rotates_the_protected_credential_and_coalesces_concurrent_renewal() {
    let (manager, store, provider) = fixture(None);
    assert_eq!(
        manager.restore().unwrap().unwrap().username,
        "developer@example.test"
    );
    std::thread::scope(|scope| {
        for _ in 0..6 {
            let manager = manager.clone();
            scope.spawn(move || {
                assert!(manager.reconnect().unwrap().is_some());
            });
        }
    });
    assert_eq!(provider.count.load(Ordering::SeqCst), 1);
    assert!(
        store
            .load()
            .unwrap()
            .unwrap()
            .expose()
            .contains("rotated-fixture")
    );
    assert!(
        !store
            .load()
            .unwrap()
            .unwrap()
            .expose()
            .contains("access-fixture")
    );
    manager.sign_out().unwrap();
    assert!(store.load().unwrap().is_none());
    assert!(manager.reconnect().unwrap().is_none());
}
#[test]
fn offline_and_interaction_required_keep_recoverable_session_state() {
    for failure in [Error::Offline, Error::SignInRequired] {
        let (manager, store, _) = fixture(Some(failure));
        assert_eq!(manager.restore(), Err(failure));
        assert!(store.load().unwrap().is_some());
        assert!(manager.cache.lock().unwrap().access.is_empty());
        manager.sign_out().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}

#[test]
fn keychain_failures_keep_rotated_credentials_and_do_not_claim_sign_out() {
    let (manager, store, _) = fixture(None);
    store.1.store(true, Ordering::SeqCst);
    assert_eq!(manager.restore(), Err(Error::Keychain));
    assert_eq!(
        manager
            .cache
            .lock()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .refresh
            .expose(),
        "rotated-fixture"
    );
    assert!(manager.cache.lock().unwrap().access.is_empty());
    store.1.store(false, Ordering::SeqCst);
    assert!(manager.reconnect().unwrap().is_some());
    store.2.store(true, Ordering::SeqCst);
    assert_eq!(manager.sign_out(), Err(Error::Keychain));
    assert!(store.load().unwrap().is_some());
}

#[test]
fn cancelled_or_old_sign_in_cannot_replace_the_existing_account() {
    let (manager, store, _) = fixture(None);
    let session = super::decode(&store.load().unwrap().unwrap()).unwrap();
    let granted = || Granted {
        session: session.clone(),
        access: Secret::new("unused"),
        expires_at: std::time::Instant::now() + Duration::from_secs(3600),
    };
    let old_generation = manager.generation();
    manager.invalidate();
    assert_eq!(
        manager.accept(granted(), &crate::Cancel::default(), old_generation, ""),
        Err(Error::Cancelled)
    );
    let cancel = crate::Cancel::default();
    cancel.cancel();
    assert_eq!(
        manager.accept(granted(), &cancel, manager.generation(), ""),
        Err(Error::Cancelled)
    );
    assert!(
        store
            .load()
            .unwrap()
            .unwrap()
            .expose()
            .contains("refresh-fixture")
    );
}

mod resources;

#[test]
fn retrying_startup_restore_keeps_a_rotated_credential_after_a_keychain_failure() {
    let (manager, store, provider) = fixture(None);
    store.1.store(true, Ordering::SeqCst);
    assert_eq!(manager.restore(), Err(Error::Keychain));
    assert!(manager.cache.lock().unwrap().pending_save.is_some());
    store.1.store(false, Ordering::SeqCst);
    assert!(manager.restore().unwrap().is_some());
    assert_eq!(
        *provider.refreshes.lock().unwrap(),
        ["refresh-fixture", "rotated-fixture"]
    );
    assert!(manager.cache.lock().unwrap().pending_save.is_none());
}

#[test]
fn a_refresh_that_goes_stale_keeps_the_rotated_credential() {
    let (manager, store, provider) = fixture(None);
    let before = manager.generation();
    // An account switch starts while the refresh is in flight.
    manager.invalidate();
    {
        let mut cache = manager.cache.lock().unwrap();
        cache.session = Some(super::decode(&store.load().unwrap().unwrap()).unwrap());
        assert_eq!(
            manager.renew(&mut cache, before, "example-api", Duration::ZERO),
            Err(Error::Cancelled)
        );
        assert!(cache.access.is_empty(), "the stale access token is dropped");
    }
    assert!(
        store
            .load()
            .unwrap()
            .unwrap()
            .expose()
            .contains("rotated-fixture")
    );
    // The switch is cancelled: the retained credential still renews.
    assert!(manager.reconnect().unwrap().is_some());
    assert_eq!(
        *provider.refreshes.lock().unwrap(),
        ["refresh-fixture", "rotated-fixture"]
    );
}

#[test]
fn committing_a_sign_in_advances_the_generation() {
    let (manager, store, _) = fixture(None);
    let session = super::decode(&store.load().unwrap().unwrap()).unwrap();
    let switching = manager.generation();
    let granted = Granted {
        session,
        access: Secret::new("unused"),
        expires_at: std::time::Instant::now() + Duration::from_secs(3600),
    };
    manager
        .accept(granted, &crate::Cancel::default(), switching, "")
        .unwrap();
    assert_ne!(manager.generation(), switching);
}
