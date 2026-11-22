//! Planning a built-in Space's config entry: whose the saved entry is, and
//! the entry and grants to save for the Space's package.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value};

use super::BUILT_IN_KEY;
use crate::manifest;

/// The hex digits of a grant identity, a SHA-256.
const IDENTITY_HEX_CHARS: usize = 64;

/// The saved entry the app may write, with whether it is already the
/// built-in one: none yet, one to adopt, or the built-in one. `None` when
/// the entry is the user's.
pub(super) fn claimed(saved: Option<&Value>) -> Option<(Map<String, Value>, bool)> {
    let Some(saved) = saved else {
        return Some((Map::new(), false));
    };
    let entry = saved.as_object()?;
    if super::is_built_in(entry) {
        return Some((entry.clone(), true));
    }
    let theirs = ["managedSource", "url", "path"]
        .iter()
        .any(|key| entry.contains_key(*key));
    (!theirs).then(|| (entry.clone(), false))
}

/// What a built-in Space's entry is planned from: its folder, its title in
/// the tab strip, and its manifest's grant identity and the names of the
/// permissions it asks for.
pub(super) struct Package<'a> {
    pub root: &'a Path,
    pub title: &'a str,
    pub identity: String,
    pub names: Vec<&'static str>,
}

impl<'a> Package<'a> {
    /// The package of the built-in Space at `root`, read from its manifest.
    pub(super) fn read(root: &'a Path, title: &'a str) -> Result<Self, String> {
        let manifest = manifest::read(root)?.ok_or("The built-in web Space has no manifest.")?;
        let names = manifest
            .permissions
            .iter()
            .map(|permission| permission.name())
            .collect();
        Ok(Self {
            root,
            title,
            identity: manifest.identity(root),
            names,
        })
    }
}

/// The entry to save for `package`. A new or adopted entry is marked built
/// in; either way each permission is granted unless the entry already
/// decided it (see [`grant`]).
pub(super) fn planned_entry(
    mut entry: Map<String, Value>,
    built_in: bool,
    package: &Package<'_>,
) -> Map<String, Value> {
    if !built_in {
        entry
            .entry("title")
            .or_insert_with(|| Value::String(package.title.to_owned()));
        entry.insert(BUILT_IN_KEY.into(), Value::Bool(true));
    }
    entry.insert(
        "path".into(),
        Value::String(package.root.to_string_lossy().into_owned()),
    );
    let permissions = entry
        .entry("permissions")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(permissions) = permissions.as_object_mut() {
        grant(permissions, &package.identity, &package.names);
    }
    entry
}

/// Gives each permission in `names` a grant under `identity`. One already
/// there is kept. One the entry decided under an earlier identity of the
/// Space, before its manifest or folder changed, carries over by name, a
/// denial winning; that earlier grant is removed. Any other is granted: the
/// Space is the app's own code.
fn grant(permissions: &mut Map<String, Value>, identity: &str, names: &[&str]) {
    let mut earlier: BTreeMap<String, bool> = BTreeMap::new();
    permissions.retain(|key, value| {
        let earlier_grant = key
            .split_once('.')
            .filter(|(owner, _)| is_identity(owner) && *owner != identity)
            .zip(value.as_bool());
        let Some(((_, name), on)) = earlier_grant else {
            return true;
        };
        *earlier.entry(name.to_owned()).or_insert(on) &= on;
        false
    });
    for name in names {
        let on = earlier.get(*name).copied().unwrap_or(true);
        permissions
            .entry(format!("{identity}.{name}"))
            .or_insert(Value::Bool(on));
    }
}

/// Whether `text` has the form of a grant identity: a SHA-256 in hex.
fn is_identity(text: &str) -> bool {
    text.len() == IDENTITY_HEX_CHARS && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}
