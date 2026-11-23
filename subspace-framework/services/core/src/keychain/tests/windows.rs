//! The real Windows Credential Manager, on request only.

use super::expect::namespace_in;
use super::memory::ACCOUNT;
use crate::Secret;
use crate::keychain::{Item, Keychain, Store, System};

const PROBE: &str = "probe";

/// Deletes the test's credentials however the test ends.
struct Cleanup<'a>(&'a Keychain, &'a System<'a>);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.0.delete(ACCOUNT);
        let _ = self.1.delete(PROBE);
    }
}

#[test]
#[ignore = "writes to this user's Windows Credential Manager, which no CI job has; \
            run on Windows with `cargo test -p subspace-core -- --ignored keychain`"]
fn a_long_secret_round_trips_through_credential_manager() {
    let service = format!("subspace-core-test-{:032x}", rand::random::<u128>());
    let store = System(&service);
    let keychain = Keychain::new(service.clone());
    let _cleanup = Cleanup(&keychain, &store);
    let pattern = format!(
        "(?i)^{}[0-9a-f]{{32}}/[0-9]+\\.{}$",
        regex::escape(&namespace_in(&service, ACCOUNT)),
        regex::escape(&service)
    );
    let chunks = || store.find(&pattern).unwrap();

    // 5,000 characters, 6,250 UTF-16 code units: seven chunks.
    let secret = Secret::new("aé€\u{1d11e}".repeat(1_250));
    keychain.save(ACCOUNT, &secret).unwrap();
    let Ok(Some(Item::Bytes(manifest))) = store.get(ACCOUNT) else {
        panic!("a split secret's credential reads back as bytes");
    };
    assert_eq!(chunks().len(), 7);
    assert_eq!(keychain.load(ACCOUNT).unwrap(), Some(secret));

    // Cut short, at an even length, the manifest is still not a password.
    store
        .set_bytes(PROBE, &manifest[..manifest.len() - 1])
        .unwrap();
    assert!(matches!(store.get(PROBE), Ok(Some(Item::Bytes(_)))));

    // A save removes the chunks of the generation it replaces.
    let short = Secret::new("fixture-token");
    keychain.save(ACCOUNT, &short).unwrap();
    assert!(matches!(store.get(ACCOUNT), Ok(Some(Item::Text(text))) if text == "fixture-token"));
    assert!(chunks().is_empty());

    // A delete sweeps every chunk of the account.
    keychain
        .save(ACCOUNT, &Secret::new("b".repeat(5_000)))
        .unwrap();
    assert_eq!(chunks().len(), 6);
    keychain.delete(ACCOUNT).unwrap();
    assert_eq!(keychain.load(ACCOUNT).unwrap(), None);
    assert!(chunks().is_empty());
}
