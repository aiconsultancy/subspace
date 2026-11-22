//! The app's secrets. Under `secrets` the page reads the ones its manifest
//! declares, which only the user sets in Settings, and the ones it saved
//! itself. Under `save-secrets` it saves its own with `secrets.set`, such as
//! a token per account it adds, and never a declared one. All live in the
//! Space's `secrets` in the private config file and go with the Space when
//! it is removed. A page's own are stored under their owner, the app's
//! install source ([`owner`]), so an update from the same source keeps them
//! and another app given the Space never reads them.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::App;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use subspace_framework::core::Secret;
use subspace_framework::core::config::{self, Config, web_boards::WebBoard};
use subspace_framework::core::settings::SettingsFile;
use subspace_framework::dashboard::Services;
use subspace_framework::dashboard::settings::FieldKind;

use super::Bridge;
use crate::manifest::{Manifest, Permission, valid_key};
pub(crate) use owner::Owner;

pub(crate) mod owner;

/// The most secrets a page may save of its own.
pub(crate) const MAX_APP_SECRETS: usize = 32;
const MAX_SECRET_CHARS: usize = 4096;
/// A page saves at most this many secrets in any [`SAVE_WINDOW`]: each save
/// rewrites the config file.
const MAX_SAVES: usize = 10;
const SAVE_WINDOW: Duration = Duration::from_secs(10);

pub(crate) const BAD_KEY: &str =
    "Use a secret key of 1 to 40 letters, digits, _ or -, starting with a letter.";
pub(crate) const DECLARED_KEY: &str = "This key belongs to one of the app's settings, which only Settings changes. Choose another key.";
pub(crate) const BAD_VALUE: &str =
    "Use a secret of 1 to 4096 characters, without control characters, or null to delete it.";
pub(crate) const TOO_MANY: &str =
    "This app has saved 32 secrets, the most it may. Delete one before saving another.";
pub(crate) const TOO_FAST: &str = "Too many secret saves. Wait a few seconds, then try again.";
pub(crate) const NOT_SHOWN: &str = "Open this Space to save a secret.";
pub(crate) const CHANGED: &str =
    "This Space changed in the config file. Review it in Settings, then try again.";
const NOT_SAVED: &str =
    "Subspace could not save the secret. Check the config file in Settings, then try again.";

/// `secrets.set(key, value)`: `None` deletes.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SecretWrite {
    pub key: String,
    pub value: Option<String>,
}

/// Whether stored `key` is a secret some app's page saved itself, rather
/// than one the user set.
pub(crate) fn is_app_secret(key: &str) -> bool {
    owner::is_owned(key)
}

/// The secrets `owner`'s app saved itself, by page key. Of a key stored in
/// several forms the owner's own wins, then the older ones.
fn own<'c>(
    manifest: &Manifest,
    owner: &Owner,
    config: &'c WebBoard,
) -> BTreeMap<&'c str, &'c Secret> {
    let mut found: BTreeMap<&str, (u8, &Secret)> = BTreeMap::new();
    let claimed = config.secrets.iter().filter_map(|(stored, secret)| {
        let (key, age) = owner.claims(stored)?;
        // An ownerless key the manifest declares is a declared secret.
        (age < 2 || declared(manifest, key).is_none()).then_some((key, age, secret))
    });
    for (key, age, secret) in claimed {
        if found.get(key).is_none_or(|(held, _)| age < *held) {
            found.insert(key, (age, secret));
        }
    }
    found
        .into_iter()
        .map(|(key, (_, secret))| (key, secret))
        .collect()
}

/// The declared secrets with a saved value, and every secret `owner`'s app
/// saved itself. A declared key wins over an own one.
pub(crate) fn readable(manifest: &Manifest, owner: &Owner, config: &WebBoard) -> Value {
    let declared = config
        .secrets
        .iter()
        .filter(|(key, _)| matches!(declared(manifest, key), Some(FieldKind::Secret)))
        .map(|(key, secret)| (key.as_str(), secret));
    let values = own(manifest, owner, config)
        .into_iter()
        .chain(declared)
        .filter_map(|(key, secret)| {
            let secret = secret.expose();
            (!secret.is_empty()).then(|| (key.to_owned(), Value::from(secret)))
        })
        .collect();
    Value::Object(values)
}

/// Why `write` may not be saved for `owner`'s app, if it may not.
pub(crate) fn check(
    manifest: &Manifest,
    owner: &Owner,
    config: &WebBoard,
    write: &SecretWrite,
) -> Result<(), &'static str> {
    if !valid_key(&write.key) {
        return Err(BAD_KEY);
    }
    if declared(manifest, &write.key).is_some() {
        return Err(DECLARED_KEY);
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
    let own = own(manifest, owner, config);
    if !own.contains_key(write.key.as_str()) && own.len() >= MAX_APP_SECRETS {
        return Err(TOO_MANY);
    }
    Ok(())
}

/// How many secrets `owner`'s app saved itself.
#[cfg(test)]
pub(crate) fn app_secret_count(manifest: &Manifest, owner: &Owner, config: &WebBoard) -> usize {
    own(manifest, owner, config).len()
}

fn declared<'m>(manifest: &'m Manifest, key: &str) -> Option<&'m FieldKind> {
    manifest
        .settings
        .iter()
        .find(|field| field.key == key)
        .map(|field| &field.kind)
}

/// The times of a page's recent saves, to hold it to [`MAX_SAVES`].
#[derive(Default)]
pub(crate) struct Saves(VecDeque<Instant>);

impl Saves {
    /// Counts a save at `now`, or refuses it past the limit.
    pub(crate) fn allow(&mut self, now: Instant) -> Result<(), &'static str> {
        while self
            .0
            .front()
            .is_some_and(|saved| now.saturating_duration_since(*saved) >= SAVE_WINDOW)
        {
            self.0.pop_front();
        }
        if self.0.len() >= MAX_SAVES {
            return Err(TOO_FAST);
        }
        self.0.push_back(now);
        Ok(())
    }
}

/// Writes `write` to board `id`'s entry in the config file, then to the
/// configuration the app runs with, so the page reads it back at once. The
/// entry in the file decides: it must still give `id` this app's folder,
/// install source and `save-secrets` grant, and its own secrets count
/// toward the limit. The write replaces the key's older stored forms.
pub(crate) fn save(
    bridge: &Bridge,
    id: &str,
    write: &SecretWrite,
    cx: &mut App,
) -> Result<(), &'static str> {
    let path = cx
        .try_global::<Services>()
        .map(|services| services.config_file.clone())
        .ok_or(NOT_SAVED)?;
    let mut file = SettingsFile::open(&path).map_err(|_| NOT_SAVED)?;
    let entry = file.web_board(id).ok_or(NOT_SAVED)?;
    let saved = persisted(id, &entry, &path).ok_or(NOT_SAVED)?;
    if !bridge.allows(Permission::SaveSecrets, &saved) || !bridge.owns(&saved, &entry) {
        return Err(CHANGED);
    }
    check(&bridge.manifest, &bridge.owner, &saved, write)?;
    let forms = bridge.owner.forms(&write.key);
    let value = write.value.as_deref();
    file.update_web_board(id, Some(written(entry, &forms, value)))
        .map_err(|_| NOT_SAVED)?;
    apply(id, &forms, value, cx);
    Ok(())
}

impl Bridge {
    /// Whether `board`, defined by raw config entry `entry`, still gives this
    /// app's page secrets the owner they have now.
    fn owns(&self, board: &WebBoard, entry: &Map<String, Value>) -> bool {
        Owner::of_entry(&self.identity, &board.source, &board.id, Some(entry)) == self.owner
    }
}

/// Board `id` as the opened file defines it. The write that follows is
/// refused if the file changes after it was opened, so both agree.
fn persisted(id: &str, entry: &Map<String, Value>, path: &Path) -> Option<WebBoard> {
    let text = json!({ "webBoards": { id: entry } }).to_string();
    config::parse(&text, path).0.web_boards.into_iter().next()
}

/// The raw entry with every stored form of one secret, `forms`, removed,
/// then `value` stored under the first, every other field kept as it was.
pub(crate) fn written(
    mut entry: Map<String, Value>,
    forms: &[String],
    value: Option<&str>,
) -> Map<String, Value> {
    let mut secrets = match entry.remove("secrets") {
        Some(Value::Object(secrets)) => secrets,
        _ => Map::new(),
    };
    for form in forms {
        secrets.remove(form);
    }
    if let (Some(key), Some(value)) = (forms.first(), value) {
        secrets.insert(key.clone(), Value::from(value));
    }
    if !secrets.is_empty() {
        entry.insert("secrets".to_owned(), Value::Object(secrets));
    }
    entry
}

fn apply(id: &str, forms: &[String], value: Option<&str>, cx: &mut App) {
    let services = cx.global_mut::<Services>();
    services.config = Arc::new(with_secret(&services.config, id, forms, value));
}

/// The running configuration with board `id`'s secret written as
/// [`written`] writes it to the file.
fn with_secret(config: &Config, id: &str, forms: &[String], value: Option<&str>) -> Config {
    let mut config = config.clone();
    let secret = value.map(Secret::new);
    if let Some(board) = config.web_boards.iter_mut().find(|board| board.id == id) {
        for form in forms {
            board.secrets.remove(form);
        }
        if let (Some(key), Some(secret)) = (forms.first(), secret) {
            board.secrets.insert(key.clone(), secret);
        }
    }
    config
}

#[cfg(test)]
mod tests;
