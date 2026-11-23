//! The real Windows Credential Manager, on request only.

use super::memory::{ACCOUNT, namespace_of};
use crate::Secret;
use crate::keychain::{Item, Keychain, Store, System};

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
    let namespace = namespace_of(ACCOUNT);
    let chunks = || store.list(&namespace).unwrap();

    // 5,000 characters, 6,250 UTF-16 code units: seven chunks.
    let secret = Secret::new("aé€\u{1d11e}".repeat(1_250));
    keychain.save(ACCOUNT, &secret).unwrap();
    assert!(matches!(store.get(ACCOUNT), Ok(Some(Item::Bytes(_)))));
    assert_eq!(chunks().len(), 7);
    assert_eq!(keychain.load(ACCOUNT).unwrap(), Some(secret));

    let short = Secret::new("fixture-token");
    keychain.save(ACCOUNT, &short).unwrap();
    assert!(matches!(store.get(ACCOUNT), Ok(Some(Item::Text(text))) if text == "fixture-token"));
    assert!(chunks().is_empty());

    keychain
        .save(ACCOUNT, &Secret::new("b".repeat(5_000)))
        .unwrap();
    assert_eq!(chunks().len(), 6);
    keychain.delete(ACCOUNT).unwrap();
    assert_eq!(keychain.load(ACCOUNT).unwrap(), None);
    assert!(chunks().is_empty());
}
