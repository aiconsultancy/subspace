//! The real Windows Credential Manager, on request only.

use std::collections::BTreeSet;

use super::memory::{ACCOUNT, MANIFEST_PREFIX, named};
use crate::Secret;
use crate::keychain::{Keychain, Store, System};

/// Deletes the test's secret however the test ends.
struct Cleanup<'a>(&'a Keychain);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(ACCOUNT);
    }
}

#[test]
#[ignore = "writes to this user's Windows Credential Manager, which no CI job has; \
            run on Windows with `cargo test -p subspace-core -- --ignored keychain`"]
fn a_long_secret_round_trips_through_credential_manager() {
    let service = format!("subspace-core-test-{:032x}", rand::random::<u128>());
    let store = System(&service);
    let keychain = Keychain::new(service.clone());
    let _cleanup = Cleanup(&keychain);
    let missing = |names: &BTreeSet<String>| names.iter().all(|name| store.get(name) == Ok(None));

    // 5,000 characters, 6,250 UTF-16 code units.
    let secret = Secret::new("aé€\u{1d11e}".repeat(1_250));
    keychain.save(ACCOUNT, &secret).unwrap();
    let primary = store.get(ACCOUNT).unwrap().unwrap();
    assert!(primary.starts_with(MANIFEST_PREFIX));
    let first = named(ACCOUNT, &primary);
    assert_eq!(keychain.load(ACCOUNT).unwrap(), Some(secret));

    keychain
        .save(ACCOUNT, &Secret::new("fixture-token"))
        .unwrap();
    assert_eq!(
        store.get(ACCOUNT).unwrap().as_deref(),
        Some("fixture-token")
    );
    assert!(missing(
        &first.into_iter().filter(|name| name != ACCOUNT).collect()
    ));

    keychain
        .save(ACCOUNT, &Secret::new("b".repeat(5_000)))
        .unwrap();
    let second = named(ACCOUNT, &store.get(ACCOUNT).unwrap().unwrap());
    keychain.delete(ACCOUNT).unwrap();
    assert_eq!(keychain.load(ACCOUNT).unwrap(), None);
    assert!(missing(&second));
}
