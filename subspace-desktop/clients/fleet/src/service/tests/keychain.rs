//! A token the secret store could not read: read again only when asked,
//! at most once every few seconds, and announced only when it changed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext};
use subspace_framework::core::Secret;

use super::{HOME, aurora, service, state, token};
use crate::service::FleetApi;
use crate::{FleetState, TokenStore};

/// A secret store that cannot be read while `locked`, as a locked
/// keychain, counting every read.
#[derive(Default)]
struct Keychain {
    locked: Arc<std::sync::atomic::AtomicBool>,
    reads: Arc<std::sync::atomic::AtomicUsize>,
}

impl subspace_framework::core::vault::Vault for Keychain {
    fn load(&self, _: &str) -> Result<Option<Secret>, subspace_framework::core::vault::VaultError> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.locked.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(subspace_framework::core::vault::VaultError);
        }
        Ok(None)
    }

    fn save(&self, _: &str, _: &Secret) -> Result<(), subspace_framework::core::vault::VaultError> {
        Err(subspace_framework::core::vault::VaultError)
    }

    fn delete(&self, _: &str) -> Result<(), subspace_framework::core::vault::VaultError> {
        Err(subspace_framework::core::vault::VaultError)
    }
}

/// A service over a locked keychain, with its lock and its read count.
fn locked_service(
    cx: &mut TestAppContext,
) -> (
    Entity<FleetApi>,
    Arc<std::sync::atomic::AtomicBool>,
    Arc<std::sync::atomic::AtomicUsize>,
) {
    let keychain = Keychain {
        locked: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        ..Keychain::default()
    };
    let (locked, reads) = (keychain.locked.clone(), keychain.reads.clone());
    let store = Arc::new(TokenStore::with_vault(Box::new(keychain)));
    (service(store, aurora(HOME), cx), locked, reads)
}

fn reads(count: &std::sync::atomic::AtomicUsize) -> usize {
    count.load(std::sync::atomic::Ordering::SeqCst)
}

/// The person's Retry at `at`, and whatever it reads.
fn retry_at(service: &Entity<FleetApi>, at: Instant, cx: &mut TestAppContext) {
    service.update(cx, |service, cx| service.reload_unreadable_at(at, cx));
    cx.run_until_parked();
}

fn revision(service: &Entity<FleetApi>, cx: &mut TestAppContext) -> u64 {
    service.read_with(cx, |service, _| service.directory_revision())
}

#[gpui_kit::test]
fn a_token_the_store_could_not_read_is_read_again_on_retry(cx: &mut TestAppContext) {
    let (service, locked, count) = locked_service(cx);
    assert_eq!(state(&service, cx), FleetState::StoreUnavailable);
    assert_eq!(reads(&count), 1);
    locked.store(false, std::sync::atomic::Ordering::SeqCst);
    service.update(cx, |service, cx| service.reload_unreadable(cx));
    // No Checking step: the fleet says what it said until the read lands.
    assert_eq!(state(&service, cx), FleetState::StoreUnavailable);
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::NoToken, "read again");
    assert_eq!(reads(&count), 2);
    let revision = self::revision(&service, cx);
    service.update(cx, |service, cx| service.reload_unreadable(cx));
    cx.run_until_parked();
    assert_eq!(
        self::revision(&service, cx),
        revision,
        "nothing to read again, no news"
    );
    assert_eq!(reads(&count), 2);
}

#[gpui_kit::test]
fn a_store_still_locked_on_retry_neither_announces_nor_reads_again_soon(cx: &mut TestAppContext) {
    // The loop a reviewer found: each announcement made the page ask again,
    // and each ask read the keychain again and announced again. A
    // follower that asks again on every announcement must now settle.
    let (service, _locked, count) = locked_service(cx);
    let revision = self::revision(&service, cx);
    let asks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let _follower = cx.update(|cx| {
        let asks = asks.clone();
        cx.observe(&service, move |service, cx| {
            if asks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 10 {
                service.update(cx, |service, cx| service.reload_unreadable(cx));
            }
        })
    });
    let start = Instant::now();
    retry_at(&service, start, cx);
    assert_eq!(reads(&count), 2, "the retry read the store once");
    assert_eq!(
        self::revision(&service, cx),
        revision,
        "still locked: no news"
    );
    assert_eq!(reads(&asks), 0, "nothing announced, so nothing asked again");

    // Asking again within the interval reads nothing, whoever asks.
    for seconds in [1, 4] {
        retry_at(&service, start + Duration::from_secs(seconds), cx);
    }
    assert_eq!(reads(&count), 2);
    retry_at(&service, start + Duration::from_secs(5), cx);
    assert_eq!(reads(&count), 3, "read again once the interval has passed");
    assert_eq!(self::revision(&service, cx), revision);
}

#[gpui_kit::test]
fn a_retry_asked_too_soon_reads_once_the_interval_ends(cx: &mut TestAppContext) {
    let (service, _locked, count) = locked_service(cx);
    let start = Instant::now();
    retry_at(&service, start, cx);
    assert_eq!(reads(&count), 2);
    for seconds in [1, 2] {
        retry_at(&service, start + Duration::from_secs(seconds), cx);
    }
    assert_eq!(reads(&count), 2, "not read again within the interval");
    cx.executor().advance_clock(Duration::from_secs(4));
    cx.run_until_parked();
    assert_eq!(reads(&count), 3, "the Retry is read once the interval ends");
    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert_eq!(reads(&count), 3, "and never again on a timer");
}

/// A keychain whose reads answer with the token saved before: a read that
/// began before a clear or a save, however late it lands.
struct Earlier {
    locked: Arc<std::sync::atomic::AtomicBool>,
    packed: String,
}

impl subspace_framework::core::vault::Vault for Earlier {
    fn load(&self, _: &str) -> Result<Option<Secret>, subspace_framework::core::vault::VaultError> {
        if self.locked.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(subspace_framework::core::vault::VaultError);
        }
        Ok(Some(Secret::new(self.packed.clone())))
    }

    fn save(&self, _: &str, _: &Secret) -> Result<(), subspace_framework::core::vault::VaultError> {
        Ok(())
    }

    fn delete(&self, _: &str) -> Result<(), subspace_framework::core::vault::VaultError> {
        Ok(())
    }
}

/// A service whose keychain was locked and now reads `earlier`, saved for
/// the fleet's address, until something replaces it.
fn unlocked_with(earlier: &str, cx: &mut TestAppContext) -> Entity<FleetApi> {
    let address = crate::FleetAddress::parse(HOME).unwrap();
    let keychain = Earlier {
        locked: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        packed: serde_json::json!({"v": 1, "endpoint": address.as_str(), "token": earlier})
            .to_string(),
    };
    let locked = keychain.locked.clone();
    let store = Arc::new(TokenStore::with_vault(Box::new(keychain)));
    let service = service(store, aurora(HOME), cx);
    assert_eq!(state(&service, cx), FleetState::StoreUnavailable);
    locked.store(false, std::sync::atomic::Ordering::SeqCst);
    service
}

#[gpui_kit::test(iterations = 40)]
fn a_retry_still_reading_never_brings_back_a_cleared_token(cx: &mut TestAppContext) {
    let service = unlocked_with("earlier", cx);
    service.update(cx, |service, cx| {
        service.reload_unreadable(cx);
        service.clear_token("aurora", cx).detach();
    });
    cx.run_until_parked();
    assert_eq!(
        state(&service, cx),
        FleetState::NoToken,
        "cleared stays cleared"
    );
    assert_eq!(token(&service, cx), None);
}

#[gpui_kit::test(iterations = 40)]
fn a_retry_still_reading_never_brings_back_a_replaced_token(cx: &mut TestAppContext) {
    let service = unlocked_with("earlier", cx);
    service.update(cx, |service, cx| {
        service.reload_unreadable(cx);
        service
            .save_token("aurora", Secret::new("newer"), cx)
            .detach();
    });
    cx.run_until_parked();
    assert_eq!(token(&service, cx).as_deref(), Some("newer"));
}
