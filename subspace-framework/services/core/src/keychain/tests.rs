mod failures;
mod layout;
mod memory;
#[cfg(windows)]
mod windows;

use super::{KeychainError, optional};

#[test]
fn store_errors_never_expose_provider_details() {
    assert_eq!(optional::<()>(Err(keyring::v1::Error::NoEntry)), Ok(None));
    assert_eq!(optional(Ok("fake")), Ok(Some("fake")));
    assert!(!KeychainError.to_string().contains("fake"));
}
