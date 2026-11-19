//! `subspace-board.json`: what a web app board is, what it may ask Subspace
//! for, and the settings it offers. It sits at the root of the app folder.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::Path;

use serde_json::{Map, Value};
use subspace_framework::dashboard::settings::FieldSpec;

mod fields;

pub(crate) const MANIFEST: &str = "subspace-board.json";
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_NAME_CHARS: usize = 60;
const MAX_VERSION_CHARS: usize = 40;

/// What the app may ask Subspace for, subject to explicit user grants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Permission {
    Notifications,
    Settings,
    Badge,
    Context,
}

impl Permission {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "notifications" => Self::Notifications,
            "settings" => Self::Settings,
            "badge" => Self::Badge,
            "context" => Self::Context,
            _ => return None,
        })
    }

    pub(crate) fn label(self) -> &'static str {
        self.descriptor().1
    }

    pub(crate) fn name(self) -> &'static str {
        self.descriptor().0
    }

    /// The manifest name and the reader label of each permission.
    fn descriptor(self) -> (&'static str, &'static str) {
        match self {
            Self::Notifications => ("notifications", "show notifications"),
            Self::Settings => ("settings", "read its settings"),
            Self::Badge => ("badge", "show a badge on its tab"),
            Self::Context => ("context", "share context with Copilot chat"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Manifest {
    pub name: String,
    pub version: String,
    pub permissions: BTreeSet<Permission>,
    pub settings: Vec<FieldSpec>,
    /// Parts of the manifest this Subspace does not understand and ignored.
    pub warnings: Vec<String>,
    fingerprint: String,
}

impl Manifest {
    pub(crate) fn grant_key(&self, root: &Path, permission: Permission) -> String {
        format!("{}.{}", self.identity(root), permission.name())
    }

    pub(crate) fn identity(&self, root: &Path) -> String {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_owned());
        fingerprint(&format!("{}\0{}", root.display(), self.fingerprint))
    }
}

fn fingerprint(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The manifest in `folder`, or `None` when the folder has none.
pub(crate) fn read(folder: &Path) -> Result<Option<Manifest>, String> {
    let file = folder.join(MANIFEST);
    let file = match std::fs::File::open(&file) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(file_error(error)),
    };
    let mut text = String::new();
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(file_error)?;
    if text.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(format!("{MANIFEST} is larger than 256 KiB."));
    }
    parse(&text).map(Some).map_err(file_error)
}

fn file_error(error: impl std::fmt::Display) -> String {
    format!("{MANIFEST}: {error}")
}

pub(crate) fn parse(text: &str) -> Result<Manifest, String> {
    let root = match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(root)) => root,
        Ok(_) => return Err("expected a JSON object.".into()),
        Err(error) => return Err(format!("invalid JSON ({error}).")),
    };
    let mut warnings = Vec::new();
    Ok(Manifest {
        name: bounded(&root, "name", MAX_NAME_CHARS)?,
        version: bounded(&root, "version", MAX_VERSION_CHARS)?,
        permissions: permissions(root.get("permissions"), &mut warnings)?,
        settings: fields::parse(root.get("settings"))?,
        warnings,
        fingerprint: fingerprint(text),
    })
}

fn bounded(root: &Map<String, Value>, key: &str, max: usize) -> Result<String, String> {
    let text = root
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if text.is_empty() || text.chars().count() > max {
        return Err(format!(
            "expected {key} to be text of 1 to {max} characters."
        ));
    }
    Ok(text.to_owned())
}

fn permissions(
    value: Option<&Value>,
    warnings: &mut Vec<String>,
) -> Result<BTreeSet<Permission>, String> {
    let mut granted = BTreeSet::new();
    for (index, entry) in array(value, "permissions")?.iter().enumerate() {
        let name = entry
            .as_str()
            .ok_or_else(|| format!("expected permissions[{index}] to be text."))?;
        match Permission::parse(name) {
            Some(permission) => {
                granted.insert(permission);
            }
            None => warnings.push(format!("Unknown permission {name:?} ignored.")),
        }
    }
    Ok(granted)
}

fn array<'v>(value: Option<&'v Value>, name: &str) -> Result<&'v [Value], String> {
    match value {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(entries)) => Ok(entries),
        Some(_) => Err(format!("expected {name} to be a list.")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_framework::dashboard::settings::FieldKind;

    use super::{Permission, parse, read};

    fn manifest(settings: serde_json::Value) -> String {
        json!({"name": "Notes", "version": "1.0.0", "settings": settings}).to_string()
    }

    #[test]
    fn a_full_manifest_parses() {
        let text = json!({
            "name": " Notes ",
            "version": "1.2.0",
            "permissions": ["notifications", "settings", "settings", "camera"],
            "settings": [
                {"key": "endpoint", "label": "API address", "type": "url", "help": "Where notes live"},
                {"key": "limit", "label": "Limit", "type": "number", "default": 10},
                {"key": "compact", "label": "Compact", "type": "boolean", "default": true},
                {"key": "theme", "label": "Theme", "type": "select", "options": ["light", "dark"], "default": "dark"},
                {"key": "token", "label": "API token", "type": "secret"}
            ],
            "future": {"ignored": true}
        })
        .to_string();
        let manifest = parse(&text).unwrap();
        assert_eq!(manifest.name, "Notes");
        assert_eq!(
            manifest.permissions.into_iter().collect::<Vec<_>>(),
            [Permission::Notifications, Permission::Settings]
        );
        assert_eq!(
            manifest.warnings,
            ["Unknown permission \"camera\" ignored."]
        );
        assert_eq!(manifest.settings.len(), 5);
        assert_eq!(
            manifest.settings[1].kind,
            FieldKind::Number {
                default: Some(10.0)
            }
        );
        assert_eq!(
            manifest.settings[0].help.as_deref(),
            Some("Where notes live")
        );
        assert_eq!(manifest.settings[4].kind, FieldKind::Secret);
    }

    #[test]
    fn unusable_manifests_say_what_is_wrong() {
        for (text, expected) in [
            ("[]".to_owned(), "JSON object"),
            ("{".to_owned(), "invalid JSON"),
            (json!({"version": "1"}).to_string(), "name"),
            (json!({"name": "N"}).to_string(), "version"),
            (
                manifest(json!([{"key": "1x", "label": "L", "type": "text"}])),
                "key",
            ),
            (
                manifest(json!([{"key": "a", "label": "L", "type": "colour"}])),
                "type",
            ),
            (
                manifest(json!([{"key": "a", "label": "L", "type": "secret", "default": "x"}])),
                "secret",
            ),
            (
                manifest(json!([{"key": "a", "label": "L", "type": "select", "options": []}])),
                "options",
            ),
            (
                manifest(
                    json!([{"key": "a", "label": "L", "type": "select", "options": ["x"], "default": "y"}]),
                ),
                "one of the options",
            ),
            (
                manifest(json!([
                    {"key": "a", "label": "L", "type": "text"},
                    {"key": "a", "label": "M", "type": "text"}
                ])),
                "twice",
            ),
            (
                json!({"name": "N", "version": "1", "permissions": "all"}).to_string(),
                "permissions",
            ),
        ] {
            let error = parse(&text).unwrap_err();
            assert!(error.contains(expected), "{text}: {error}");
        }
    }

    #[test]
    fn a_folder_without_a_manifest_has_none() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(read(folder.path()), Ok(None));
        std::fs::write(folder.path().join("subspace-board.json"), "{}").unwrap();
        assert!(
            read(folder.path())
                .unwrap_err()
                .starts_with("subspace-board.json")
        );
    }
}
