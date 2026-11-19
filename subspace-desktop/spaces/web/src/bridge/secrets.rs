//! The app's secrets under the `secrets` permission: the ones its manifest
//! declares, which the user can also set in Settings, and the ones the page
//! saves itself with `secrets.set`, such as a token per account it adds.
//! Both live in the Space's `secrets` in the private config file and go
//! with the Space when it is removed.

use std::sync::Arc;

use gpui_kit::App;
use serde::Deserialize;
use serde_json::{Map, Value};
use subspace_framework::core::Secret;
use subspace_framework::core::config::Config;
use subspace_framework::core::config::web_boards::WebBoard;
use subspace_framework::core::settings::SettingsFile;
use subspace_framework::dashboard::Services;
use subspace_framework::dashboard::settings::FieldKind;

use super::Effect;
use super::protocol::Operation;
use crate::manifest::{Manifest, valid_key};

/// The most secrets a page may save beyond the ones its manifest declares.
pub(crate) const MAX_APP_SECRETS: usize = 32;
const MAX_SECRET_CHARS: usize = 4096;

pub(crate) const BAD_KEY: &str =
    "Use a secret key of 1 to 40 letters, digits, _ or -, starting with a letter.";
pub(crate) const SETTING_KEY: &str =
    "This key is one of the app's settings, not a secret. Choose another key.";
pub(crate) const BAD_VALUE: &str =
    "Use a secret of 1 to 4096 characters, without control characters, or null to delete it.";
pub(crate) const TOO_MANY: &str =
    "This app has saved 32 secrets, the most it may. Delete one before saving another.";
pub(crate) const NOT_SHOWN: &str = "Open this Space to save a secret.";
const NOT_SAVED: &str =
    "Subspace could not save the secret. Check the config file in Settings, then try again.";

/// `secrets.set(key, value)`: `None` deletes.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SecretWrite {
    pub key: String,
    pub value: Option<String>,
}

/// The declared secrets with a saved value, and every secret the page saved.
pub(crate) fn readable(manifest: &Manifest, config: &WebBoard) -> Value {
    let values = config
        .secrets
        .iter()
        .filter(|(key, _)| match declared(manifest, key) {
            Some(kind) => matches!(kind, FieldKind::Secret),
            None => true,
        })
        .filter_map(|(key, secret)| {
            let secret = secret.expose();
            (!secret.is_empty()).then(|| (key.clone(), Value::from(secret)))
        })
        .collect();
    Value::Object(values)
}

/// Why `write` may not be saved, if it may not.
pub(crate) fn check(
    manifest: &Manifest,
    config: &WebBoard,
    write: &SecretWrite,
) -> Result<(), &'static str> {
    if !valid_key(&write.key) {
        return Err(BAD_KEY);
    }
    if declared(manifest, &write.key).is_some_and(|kind| !matches!(kind, FieldKind::Secret)) {
        return Err(SETTING_KEY);
    }
    let Some(value) = &write.value else {
        return Ok(());
    };
    if value.is_empty()
        || value.chars().count() > MAX_SECRET_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(BAD_VALUE);
    }
    let new_app_secret =
        declared(manifest, &write.key).is_none() && !config.secrets.contains_key(&write.key);
    if new_app_secret && app_secret_count(manifest, config) >= MAX_APP_SECRETS {
        return Err(TOO_MANY);
    }
    Ok(())
}

/// How many saved secrets the manifest does not declare.
pub(crate) fn app_secret_count(manifest: &Manifest, config: &WebBoard) -> usize {
    config
        .secrets
        .keys()
        .filter(|key| declared(manifest, key).is_none())
        .count()
}

fn declared<'m>(manifest: &'m Manifest, key: &str) -> Option<&'m FieldKind> {
    manifest
        .settings
        .iter()
        .find(|field| field.key == key)
        .map(|field| &field.kind)
}

/// Writes `write` to board `id`'s entry in the config file, then to the
/// configuration the app runs with, so the page reads it back at once
/// without reloading any Space.
pub(crate) fn save(id: &str, write: &SecretWrite, cx: &mut App) -> Result<(), &'static str> {
    let path = cx
        .try_global::<Services>()
        .map(|services| services.config_file.clone())
        .ok_or(NOT_SAVED)?;
    let mut file = SettingsFile::open(&path).map_err(|_| NOT_SAVED)?;
    let entry = file.web_board(id).ok_or(NOT_SAVED)?;
    file.update_web_board(id, Some(written(entry, write)))
        .map_err(|_| NOT_SAVED)?;
    apply(id, write, cx);
    Ok(())
}

/// The raw entry with `write` applied to its `secrets`, every other field
/// kept as it was.
pub(crate) fn written(mut entry: Map<String, Value>, write: &SecretWrite) -> Map<String, Value> {
    let mut secrets = match entry.remove("secrets") {
        Some(Value::Object(secrets)) => secrets,
        _ => Map::new(),
    };
    match &write.value {
        Some(value) => secrets.insert(write.key.clone(), Value::from(value.as_str())),
        None => secrets.remove(&write.key),
    };
    if !secrets.is_empty() {
        entry.insert("secrets".to_owned(), Value::Object(secrets));
    }
    entry
}

fn apply(id: &str, write: &SecretWrite, cx: &mut App) {
    let services = cx.global_mut::<Services>();
    services.config = Arc::new(with_secret(&services.config, id, write));
}

/// The running configuration with `write` applied to board `id`.
fn with_secret(config: &Config, id: &str, write: &SecretWrite) -> Config {
    let mut config = config.clone();
    let secret = write.value.clone().map(Secret::new);
    if let Some(board) = config.web_boards.iter_mut().find(|board| board.id == id) {
        match secret {
            Some(secret) => board.secrets.insert(write.key.clone(), secret),
            None => board.secrets.remove(&write.key),
        };
    }
    config
}

/// `secrets.get` and a checked `secrets.set`, for an allowed page.
pub(crate) fn call(
    manifest: &Manifest,
    config: &WebBoard,
    operation: &Operation,
) -> Result<(Value, Effect), &'static str> {
    match operation {
        Operation::SecretsSet(write) => {
            check(manifest, config, write)?;
            Ok((Value::Null, Effect::SaveSecret(write.clone())))
        }
        _ => Ok((readable(manifest, config), Effect::None)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;
    use subspace_framework::core::Secret;
    use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

    use super::{
        BAD_KEY, BAD_VALUE, MAX_APP_SECRETS, SETTING_KEY, SecretWrite, TOO_MANY, app_secret_count,
        check, readable, written,
    };
    use crate::manifest::parse;

    fn manifest() -> crate::manifest::Manifest {
        parse(
            &json!({"name": "Fleet", "version": "1", "permissions": ["secrets"],
                "settings": [{"key": "token", "label": "Token", "type": "secret"},
                    {"key": "endpoint", "label": "Endpoint", "type": "url"}]})
            .to_string(),
        )
        .unwrap()
    }

    fn board(secrets: &[(&str, &str)]) -> WebBoard {
        WebBoard {
            id: "fleet".into(),
            title: "Fleet".into(),
            source: WebBoardSource::Url("https://fleet.example".into()),
            settings: BTreeMap::new(),
            secrets: secrets
                .iter()
                .map(|(key, value)| ((*key).to_owned(), Secret::new(*value)))
                .collect(),
            permissions: BTreeMap::new(),
        }
    }

    fn write(key: &str, value: Option<&str>) -> SecretWrite {
        SecretWrite {
            key: key.into(),
            value: value.map(str::to_owned),
        }
    }

    #[test]
    fn the_page_reads_declared_and_its_own_secrets_but_no_setting_keys() {
        let board = board(&[
            ("token", "declared"),
            ("fleet-a", "own"),
            ("endpoint", "legacy"),
            ("empty", ""),
        ]);
        assert_eq!(
            readable(&manifest(), &board),
            json!({"fleet-a": "own", "token": "declared"})
        );
        assert_eq!(app_secret_count(&manifest(), &board), 2);
    }

    #[test]
    fn keys_and_values_are_checked_and_setting_keys_refused() {
        let (manifest, board) = (manifest(), board(&[]));
        let check = |write: SecretWrite| check(&manifest, &board, &write);
        assert_eq!(check(write("fleet-a", Some("t"))), Ok(()));
        assert_eq!(check(write("token", Some("t"))), Ok(()));
        assert_eq!(check(write("fleet-a", None)), Ok(()));
        assert_eq!(check(write("1fleet", Some("t"))), Err(BAD_KEY));
        assert_eq!(check(write(&"a".repeat(41), Some("t"))), Err(BAD_KEY));
        assert_eq!(check(write("endpoint", Some("t"))), Err(SETTING_KEY));
        assert_eq!(check(write("endpoint", None)), Err(SETTING_KEY));
        assert_eq!(check(write("fleet-a", Some(""))), Err(BAD_VALUE));
        assert_eq!(check(write("fleet-a", Some("a\nb"))), Err(BAD_VALUE));
        assert_eq!(
            check(write("fleet-a", Some(&"x".repeat(4097)))),
            Err(BAD_VALUE)
        );
        assert_eq!(check(write("fleet-a", Some(&"界".repeat(4096)))), Ok(()));
    }

    #[test]
    fn at_most_32_own_secrets_and_the_declared_ones_do_not_count() {
        let keys: Vec<String> = (0..MAX_APP_SECRETS).map(|n| format!("fleet-{n}")).collect();
        let mut pairs: Vec<(&str, &str)> = keys.iter().map(|key| (key.as_str(), "t")).collect();
        pairs.push(("token", "declared"));
        let (manifest, board) = (manifest(), board(&pairs));
        assert_eq!(app_secret_count(&manifest, &board), MAX_APP_SECRETS);
        let check = |write: SecretWrite| check(&manifest, &board, &write);
        assert_eq!(check(write("fleet-new", Some("t"))), Err(TOO_MANY));
        assert_eq!(check(write("fleet-0", Some("replaced"))), Ok(()));
        assert_eq!(check(write("fleet-0", None)), Ok(()));
        assert_eq!(check(write("token", Some("t"))), Ok(()));
    }

    #[gpui_kit::test]
    fn a_saved_secret_reaches_the_file_and_the_running_config(cx: &mut gpui_kit::TestAppContext) {
        use std::sync::Arc;
        use subspace_framework::core::{config, credentials::Credentials};
        use subspace_framework::dashboard::Services;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let text = json!({"webBoards": {"fleet": {"url": "https://fleet.example",
            "secrets": {"token": "t"}}}, "futureSetting": 42});
        std::fs::write(&path, text.to_string()).unwrap();
        cx.update(|cx| {
            let config = config::load(&path).0;
            cx.set_global(Services {
                credentials: Arc::new(Credentials::from_system(&config)),
                config: Arc::new(config),
                tools: Default::default(),
                search: None,
                config_file: path.clone(),
            });
            assert_eq!(
                super::save("fleet", &write("fleet-a", Some("new")), cx),
                Ok(())
            );
            let running = &cx.global::<Services>().config.web_boards[0];
            assert_eq!(running.secrets["fleet-a"].expose(), "new");
            assert_eq!(super::save("fleet", &write("token", None), cx), Ok(()));
            assert!(super::save("missing", &write("fleet-a", Some("t")), cx).is_err());
        });
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved["webBoards"]["fleet"]["secrets"],
            json!({"fleet-a": "new"})
        );
        assert_eq!(saved["futureSetting"], 42);
    }

    #[test]
    fn a_write_changes_only_the_secret_and_keeps_the_entry() {
        let entry = json!({"path": "/apps/fleet", "settings": {"endpoint": "https://a.example"},
            "secrets": {"token": "t"}, "permissions": {"x.secrets": true}});
        let entry = entry.as_object().unwrap().clone();
        let added = written(entry.clone(), &write("fleet-a", Some("new")));
        assert_eq!(added["secrets"], json!({"token": "t", "fleet-a": "new"}));
        assert_eq!(added["settings"], entry["settings"]);
        assert_eq!(added["permissions"], entry["permissions"]);
        let removed = written(entry, &write("token", None));
        assert!(!removed.contains_key("secrets"));
        assert_eq!(removed["path"], "/apps/fleet");
    }
}
