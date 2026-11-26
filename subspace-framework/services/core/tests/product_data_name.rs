//! A fictional app, `example-app`, names its own data directory. The setting
//! is process-wide, so this is the only test in the binary.

use subspace_core::credentials::signed_in::keychain_service;
use subspace_core::{config, state};
use subspace_product::{Channel, DataNameError, configure_channel, configure_data_name};

#[test]
fn state_config_cache_log_and_keychain_names_follow_the_app() {
    assert_eq!(configure_data_name("example-app"), Ok(()));
    assert!(configure_channel(Channel::Beta));
    assert_eq!(
        configure_data_name("other"),
        Err(DataNameError::Configured("example-app".into()))
    );
    assert!(!configure_channel(Channel::Nightly));

    assert_eq!(state::directory_name(), "example-app-beta");
    for file in [
        state::path().unwrap(),
        state::sibling("github-cache.json").unwrap(),
        state::sibling("logs").unwrap(),
        config::path().unwrap(),
    ] {
        assert!(
            file.components()
                .any(|part| part.as_os_str() == "example-app-beta"),
            "{file:?}"
        );
    }
    assert_eq!(keychain_service(), "example-app-beta.github");
}
