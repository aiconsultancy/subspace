mod failures;
mod layout;
mod legacy;
mod memory;
mod names;
mod steps;
#[cfg(windows)]
mod windows;

use std::collections::HashMap;

use regex::Regex;

use super::{Item, KeychainError, account_of, item, optional, target_pattern};

#[test]
fn store_errors_never_expose_provider_details() {
    assert_eq!(optional::<()>(Err(keyring::v1::Error::NoEntry)), Ok(None));
    assert_eq!(optional(Ok("fake")), Ok(Some("fake")));
    assert!(!KeychainError.to_string().contains("fake"));
}

#[test]
fn bytes_the_store_cannot_read_as_text_are_an_item_of_bytes() {
    let bytes = item(Err(keyring::v1::Error::BadEncoding(vec![0xff, 1, 2])));
    assert!(matches!(bytes, Ok(Some(Item::Bytes(bytes))) if bytes == [0xff, 1, 2]));
    assert!(
        matches!(item(Ok("fixture".to_owned())), Ok(Some(Item::Text(text))) if text == "fixture")
    );
    assert!(matches!(item(Err(keyring::v1::Error::NoEntry)), Ok(None)));
    assert!(item(Err(keyring::v1::Error::NoDefaultStore)).is_err());
}

#[test]
fn the_windows_search_finds_only_the_accounts_chunks_under_the_service() {
    let pattern = Regex::new(&target_pattern("subspace-keychain#00ff/", "app.example")).unwrap();
    for target in [
        "subspace-keychain#00ff/0123/0.app.example",
        "SUBSPACE-KEYCHAIN#00FF/0123/0.APP.EXAMPLE",
    ] {
        assert!(pattern.is_match(target));
    }
    for target in [
        "session.app.example",
        "subspace-keychain#00fe/0123/0.app.example",
        "subspace-keychain#00ff/0123/0.app.example.other",
        "subspace-keychain#00ff/0123/0.appxexample",
        "xsubspace-keychain#00ff/0123/0.app.example",
    ] {
        assert!(!pattern.is_match(target));
    }
}

#[test]
fn a_found_credential_is_named_by_its_target_without_the_service() {
    let attributes = |target: &str| {
        Ok(HashMap::from([(
            "target_name".to_owned(),
            target.to_owned(),
        )]))
    };
    assert_eq!(
        account_of(
            attributes("subspace-keychain#00ff/0123/0.app.example"),
            "app.example"
        ),
        Ok("subspace-keychain#00ff/0123/0".to_owned())
    );
    assert_eq!(
        account_of(attributes("x"), "app.example"),
        Err(KeychainError)
    );
    assert_eq!(
        account_of(Ok(HashMap::new()), "app.example"),
        Err(KeychainError)
    );
    assert_eq!(
        account_of(Err(keyring::v1::Error::NoEntry), "app.example"),
        Err(KeychainError)
    );
}
