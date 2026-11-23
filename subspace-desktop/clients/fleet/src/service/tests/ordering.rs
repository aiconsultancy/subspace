//! The token store's one ordered path: reads, saves and clears of a
//! fleet's token change the keychain and the service in the order they
//! were asked for, a clear stays cleared, a failed save settles, and a
//! Retry asked too soon reads each fleet it covered once, when that
//! fleet's own interval ends.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, Entity, TestAppContext};
use subspace_framework::core::Secret;
use subspace_framework::core::vault::{Vault, VaultError};

use super::{HOME, aurora, service, state, token};
use crate::service::FleetApi;
use crate::{FleetAddress, FleetRecord, FleetState, TokenStore};

#[derive(Default)]
struct Held {
    locked: bool,
    fail_saves: bool,
    saved: HashMap<String, String>,
    reads: HashMap<String, usize>,
}

/// A keychain that can be locked, refuse saves, and count reads per
/// account. Deleting always works.
#[derive(Clone, Default)]
struct Shelf(Arc<Mutex<Held>>);

impl Shelf {
    fn with(&self, change: impl FnOnce(&mut Held)) {
        change(&mut self.0.lock().unwrap());
    }

    fn reads(&self, fleet: &str) -> usize {
        let held = self.0.lock().unwrap();
        held.reads
            .get(&format!("fleet-api:{fleet}"))
            .copied()
            .unwrap_or_default()
    }

    fn empty(&self) -> bool {
        self.0.lock().unwrap().saved.is_empty()
    }
}

impl Vault for Shelf {
    fn load(&self, account: &str) -> Result<Option<Secret>, VaultError> {
        let mut held = self.0.lock().unwrap();
        *held.reads.entry(account.to_owned()).or_default() += 1;
        if held.locked {
            return Err(VaultError);
        }
        Ok(held.saved.get(account).cloned().map(Secret::new))
    }

    fn save(&self, account: &str, secret: &Secret) -> Result<(), VaultError> {
        let mut held = self.0.lock().unwrap();
        if held.locked || held.fail_saves {
            return Err(VaultError);
        }
        held.saved
            .insert(account.to_owned(), secret.expose().to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), VaultError> {
        self.0.lock().unwrap().saved.remove(account);
        Ok(())
    }
}

fn store(shelf: &Shelf) -> Arc<TokenStore> {
    Arc::new(TokenStore::with_vault(Box::new(shelf.clone())))
}

/// A service whose keychain holds `earlier` for aurora but was locked at
/// the first read, and is now unlocked.
fn unlocked(shelf: &Shelf, cx: &mut TestAppContext) -> Entity<FleetApi> {
    let store = store(shelf);
    let address = FleetAddress::parse(HOME).unwrap();
    store
        .save("aurora", &address, &Secret::new("earlier"))
        .unwrap();
    shelf.with(|held| held.locked = true);
    let service = service(store, aurora(HOME), cx);
    assert_eq!(state(&service, cx), FleetState::StoreUnavailable);
    shelf.with(|held| held.locked = false);
    service
}

/// Long after any read, so a Retry reads at once.
fn later() -> Instant {
    Instant::now() + Duration::from_secs(3600)
}

#[gpui_kit::test(iterations = 40)]
fn a_retry_after_a_clear_never_brings_the_token_back(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let service = unlocked(&shelf, cx);
    service.update(cx, |service, cx| {
        service.clear_token("aurora", cx).detach();
        service.reload_unreadable_at(later(), cx);
    });
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::NoToken);
    assert_eq!(token(&service, cx), None);
    assert!(shelf.empty(), "nor in the keychain");
}

#[gpui_kit::test(iterations = 40)]
fn a_clear_after_a_retry_stays_cleared(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let service = unlocked(&shelf, cx);
    service.update(cx, |service, cx| {
        service.reload_unreadable_at(later(), cx);
        service.clear_token("aurora", cx).detach();
    });
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::NoToken);
    assert!(shelf.empty());
}

#[gpui_kit::test(iterations = 40)]
fn a_clear_stops_the_token_at_once(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let service = service(store(&shelf), aurora(HOME), cx);
    let saved = service.update(cx, |service, cx| {
        service.save_token("aurora", Secret::new("first"), cx)
    });
    cx.run_until_parked();
    assert!(cx.foreground_executor().block_test(saved).is_ok());
    service.update(cx, |service, cx| service.clear_token("aurora", cx).detach());
    assert_eq!(token(&service, cx), None, "before the delete lands");
    cx.run_until_parked();
    assert!(shelf.empty());
}

#[gpui_kit::test(iterations = 40)]
fn a_save_then_a_clear_leave_the_keychain_empty_whatever_runs_first(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let service = service(store(&shelf), aurora(HOME), cx);
    service.update(cx, |service, cx| {
        service
            .save_token("aurora", Secret::new("newer"), cx)
            .detach();
        service.clear_token("aurora", cx).detach();
    });
    cx.run_until_parked();
    assert_eq!(state(&service, cx), FleetState::NoToken);
    assert!(shelf.empty(), "the save never writes after the clear");
}

#[gpui_kit::test(iterations = 40)]
fn a_clear_then_a_save_keep_the_new_token(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    let service = unlocked(&shelf, cx);
    service.update(cx, |service, cx| {
        service.clear_token("aurora", cx).detach();
        service
            .save_token("aurora", Secret::new("newer"), cx)
            .detach();
    });
    cx.run_until_parked();
    assert_eq!(token(&service, cx).as_deref(), Some("newer"));
    assert!(!shelf.empty());
}

#[gpui_kit::test(iterations = 40)]
fn a_failed_save_while_the_first_read_runs_settles_from_the_keychain(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    shelf.with(|held| held.fail_saves = true);
    let service = cx.new(|_| FleetApi::new(store(&shelf)));
    let save = service.update(cx, |service, cx| {
        service.set_fleets(aurora(HOME), cx);
        service.save_token("aurora", Secret::new("refused"), cx)
    });
    cx.run_until_parked();
    assert!(cx.foreground_executor().block_test(save).is_err());
    assert_eq!(
        state(&service, cx),
        FleetState::NoToken,
        "never Checking for ever"
    );
}

#[gpui_kit::test]
fn a_retry_reads_each_fleet_it_covered_once_its_own_interval_ends(cx: &mut TestAppContext) {
    let shelf = Shelf::default();
    shelf.with(|held| held.locked = true);
    let mut fleets = aurora(HOME);
    fleets.push(FleetRecord::new(
        "borealis",
        "Borealis",
        Some(HOME.to_owned()),
    ));
    fleets.push(FleetRecord::new("cygnus", "Cygnus", Some(HOME.to_owned())));
    let service = service(store(&shelf), fleets, cx);
    let start = Instant::now();
    service.update(cx, |service, _| {
        let last = &mut service.reads.last;
        last.insert("aurora".into(), start);
        last.insert("borealis".into(), start + Duration::from_secs(2));
        last.insert("cygnus".into(), start + Duration::from_secs(2));
    });
    let reads = |shelf: &Shelf| {
        [
            shelf.reads("aurora"),
            shelf.reads("borealis"),
            shelf.reads("cygnus"),
        ]
    };
    let before = reads(&shelf);
    service.update(cx, |service, cx| {
        service.reload_unreadable_at(start + Duration::from_secs(3), cx);
        // Cygnus is cleared before its turn: it is never read for this Retry.
        service.clear_token("cygnus", cx).detach();
    });
    cx.run_until_parked();
    let cygnus = shelf.reads("cygnus");
    assert_eq!(reads(&shelf)[..2], before[..2], "too soon for both");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        reads(&shelf)[..2],
        [before[0] + 1, before[1]],
        "aurora at its second 5"
    );
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        reads(&shelf)[..2],
        [before[0] + 1, before[1] + 1],
        "borealis at its second 7"
    );
    cx.executor().advance_clock(Duration::from_secs(60));
    cx.run_until_parked();
    assert_eq!(
        reads(&shelf),
        [before[0] + 1, before[1] + 1, cygnus],
        "once each"
    );
}
