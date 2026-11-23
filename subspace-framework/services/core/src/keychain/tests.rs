mod expect;
mod failures;
mod layout;
mod legacy;
mod memory;
mod names;
mod steps;
mod sweep;
#[cfg(windows)]
mod windows;

use std::collections::HashMap;

use super::{Item, KeychainError, item, optional, target_of};

#[test]
fn store_errors_never_expose_provider_details() {
    assert_eq!(optional::<()>(Err(keyring::v1::Error::NoEntry)), Ok(None));
    assert_eq!(optional(Ok("fake")), Ok(Some("fake")));
    assert!(!KeychainError.to_string().contains("fake"));
}

#[test]
fn bytes_the_store_cannot_read_as_text_are_an_item_of_bytes() {
    let bytes = item(Err(keyring::v1::Error::BadEncoding(vec![0xff, 1, 2])));
    assert!(matches!(bytes, Ok(Some(Item::Bytes(bytes))) if *bytes == [0xff, 1, 2]));
    let text = item(Ok("fixture".to_owned()));
    assert!(matches!(text, Ok(Some(Item::Text(text))) if text == "fixture"));
    assert!(matches!(item(Err(keyring::v1::Error::NoEntry)), Ok(None)));
    assert!(item(Err(keyring::v1::Error::NoDefaultStore)).is_err());
}

#[test]
fn a_found_credential_is_named_by_its_target_or_left_out() {
    let target = "subspace-keychain#00ff/0123/0.app.example".to_owned();
    let attributes = HashMap::from([("target_name".to_owned(), target.clone())]);
    assert_eq!(target_of(Ok(attributes)), Some(target));
    assert_eq!(target_of(Ok(HashMap::new())), None);
    assert_eq!(target_of(Err(keyring::v1::Error::NoEntry)), None);
}
