//! What the bridge lets a page do with the app's secrets.

use std::time::{Duration, Instant};

use serde_json::json;
use subspace_framework::core::Secret;

use super::super::{
    DENIED, Effect,
    protocol::Operation,
    secrets::{self, SecretWrite},
};
use super::{fixture, grant};
use crate::manifest::Permission;

#[test]
fn secrets_are_the_declared_ones_with_a_value_and_this_apps_own() {
    let (_root, mut config, mut bridge) = fixture();
    let now = Instant::now();
    assert!(
        bridge
            .call(&Operation::Secrets, &config, false, now)
            .is_err()
    );
    grant(&mut config, &bridge, Permission::Settings);
    grant(&mut config, &bridge, Permission::SaveSecrets);
    assert!(
        bridge
            .call(&Operation::Secrets, &config, false, now)
            .is_err()
    );
    grant(&mut config, &bridge, Permission::Secrets);
    config
        .secrets
        .insert("limit".into(), Secret::new("declared-as-a-number"));
    let mine = bridge.owner.stored_key("mine");
    config.secrets.insert(mine, Secret::new("own"));
    let (value, _) = bridge
        .call(&Operation::Secrets, &config, false, now)
        .unwrap();
    // "limit" is a declared setting, "blank" is empty, "unset" has no value,
    // and "undeclared" has no owner: another app in the Space may have
    // saved it. "mine" is a secret this app saved itself.
    assert_eq!(value, json!({"mine": "own", "token": "secret"}));
}

fn set(key: &str, value: Option<&str>) -> Operation {
    Operation::SecretsSet(SecretWrite {
        key: key.into(),
        value: value.map(str::to_owned),
    })
}

#[test]
fn saving_needs_its_own_grant_and_never_touches_declared_secrets() {
    let (_root, mut config, mut bridge) = fixture();
    let now = Instant::now();
    grant(&mut config, &bridge, Permission::Secrets);
    for operation in [
        set("fleet-a", Some("t")),
        set("token", Some("t")),
        set("token", None),
    ] {
        assert_eq!(
            bridge.call(&operation, &config, false, now).err(),
            Some(DENIED)
        );
    }
    grant(&mut config, &bridge, Permission::SaveSecrets);
    assert!(matches!(
        bridge.call(&set("fleet-a", Some("t")), &config, false, now),
        Ok((_, Effect::SaveSecret(write))) if write.key == "fleet-a"
    ));
    for operation in [
        set("token", Some("t")),
        set("token", None),
        set("limit", Some("t")),
    ] {
        assert_eq!(
            bridge.call(&operation, &config, false, now).err(),
            Some(secrets::DECLARED_KEY)
        );
    }
}

#[test]
fn secret_saves_are_rate_limited() {
    let (_root, mut config, mut bridge) = fixture();
    grant(&mut config, &bridge, Permission::SaveSecrets);
    let now = Instant::now();
    for n in 0..10 {
        let value = n.to_string();
        assert!(
            bridge
                .call(&set("fleet-a", Some(&value)), &config, false, now)
                .is_ok()
        );
    }
    assert_eq!(
        bridge
            .call(&set("fleet-a", None), &config, false, now)
            .err(),
        Some(secrets::TOO_FAST)
    );
    // Ten seconds on, the page may save again; a refused write never counts.
    let later = now + Duration::from_secs(10);
    for _ in 0..5 {
        assert!(
            bridge
                .call(&set("token", None), &config, false, later)
                .is_err()
        );
    }
    for _ in 0..10 {
        assert!(
            bridge
                .call(&set("fleet-a", None), &config, false, later)
                .is_ok()
        );
    }
}
