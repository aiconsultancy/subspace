//! Settings a board declares at run time, such as from a web board's
//! manifest. They live in the board's own config section: values under
//! `settings` and credentials under `secrets`.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{SettingsError, TokenEdit};

const VALUES: &str = "settings";
const SECRETS: &str = "secrets";

/// Changes to one board's own settings.
#[derive(Clone, Default)]
pub struct BoardFieldEdits {
    /// The board's config section, such as `["webBoards", "notes"]`.
    pub section: Vec<String>,
    /// A value to write, or `None` to remove the key.
    pub values: BTreeMap<String, Option<Value>>,
    pub secrets: BTreeMap<String, TokenEdit>,
    pub permissions: BTreeMap<String, bool>,
}

/// The saved values of the board at `section`.
pub(super) fn values(root: &Map<String, Value>, section: &[String]) -> Map<String, Value> {
    child(root, section, VALUES).cloned().unwrap_or_default()
}

/// Whether the board at `section` has a saved secret `key`. The value
/// itself never leaves the settings file.
pub(super) fn secret_present(root: &Map<String, Value>, section: &[String], key: &str) -> bool {
    child(root, section, SECRETS).is_some_and(|secrets| secrets.contains_key(key))
}

pub(super) fn secret_keys(root: &Map<String, Value>, section: &[String]) -> Vec<String> {
    child(root, section, SECRETS)
        .map(|secrets| secrets.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn permission(root: &Map<String, Value>, section: &[String], key: &str) -> bool {
    child(root, section, "permissions")
        .and_then(|permissions| permissions.get(key))
        .and_then(Value::as_bool)
        == Some(true)
}

fn child<'r>(
    root: &'r Map<String, Value>,
    section: &[String],
    name: &str,
) -> Option<&'r Map<String, Value>> {
    let mut object = root;
    for key in section.iter().map(String::as_str).chain([name]) {
        object = object.get(key)?.as_object()?;
    }
    Some(object)
}

pub(super) fn validate(edits: &[BoardFieldEdits]) -> Result<(), SettingsError> {
    let blank = edits.iter().flat_map(|edit| edit.secrets.values()).any(
        |edit| matches!(edit, TokenEdit::Replace(secret) if secret.expose().trim().is_empty()),
    );
    let sectionless = edits.iter().any(|edit| edit.section.is_empty());
    if blank || sectionless {
        return Err(SettingsError::Validation {
            field: "board settings",
            reason: "a replacement secret cannot be blank",
        });
    }
    Ok(())
}

/// Apply `edits` to the config file's `root`. A nested board section, such
/// as `webBoards.notes`, must already exist: a board removed meanwhile is
/// not recreated. A Space's own top-level section, such as `tsxSpaces`, is
/// created on its first save.
pub(super) fn write(
    root: &mut Map<String, Value>,
    edits: &[BoardFieldEdits],
) -> Result<(), SettingsError> {
    for edit in edits {
        if let [key] = edit.section.as_slice() {
            root.entry(key.clone())
                .or_insert_with(|| Value::Object(Map::new()));
        }
        let board = object_at(root, &edit.section).ok_or(SettingsError::Validation {
            field: "board settings",
            reason: "the board is no longer in the config file",
        })?;
        apply(
            board,
            VALUES,
            edit.values.iter().map(|(key, value)| (key, value.clone())),
        );
        apply(
            board,
            SECRETS,
            edit.secrets.iter().filter_map(|(key, edit)| match edit {
                TokenEdit::Keep => None,
                TokenEdit::Replace(secret) => Some((key, Some(secret.expose().into()))),
                TokenEdit::Clear => Some((key, None)),
            }),
        );
        apply(
            board,
            "permissions",
            edit.permissions
                .iter()
                .map(|(key, on)| (key, Some(Value::Bool(*on)))),
        );
    }
    Ok(())
}

/// Set or remove keys of `board.name`, leaving out the object once empty.
fn apply<'k>(
    board: &mut Map<String, Value>,
    name: &str,
    changes: impl Iterator<Item = (&'k String, Option<Value>)>,
) {
    let mut object = board
        .remove(name)
        .and_then(|value| match value {
            Value::Object(object) => Some(object),
            _ => None,
        })
        .unwrap_or_default();
    for (key, change) in changes {
        match change {
            Some(value) => object.insert(key.clone(), value),
            None => object.remove(key),
        };
    }
    if !object.is_empty() {
        board.insert(name.to_owned(), Value::Object(object));
    }
}

fn object_at<'r>(
    root: &'r mut Map<String, Value>,
    path: &[String],
) -> Option<&'r mut Map<String, Value>> {
    path.iter()
        .try_fold(root, |object, key| object.get_mut(key)?.as_object_mut())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Map, Value, json};

    use super::{BoardFieldEdits, secret_keys, secret_present, validate, values, write};
    use crate::{Secret, settings::TokenEdit};

    fn root() -> Map<String, Value> {
        match json!({"webBoards": {"notes": {
            "path": "/apps/notes",
            "settings": {"endpoint": "https://old.example", "limit": 3},
            "secrets": {"token": "old"}
        }}, "github": {"token": "gh"}})
        {
            Value::Object(root) => root,
            _ => unreachable!(),
        }
    }

    fn section() -> Vec<String> {
        vec!["webBoards".into(), "notes".into()]
    }

    #[test]
    fn values_and_secret_presence_are_read_from_the_section() {
        let root = root();
        assert_eq!(values(&root, &section())["limit"], json!(3));
        assert!(secret_present(&root, &section(), "token"));
        assert!(!secret_present(&root, &section(), "other"));
        assert!(values(&root, &["webBoards".into(), "missing".into()]).is_empty());
        assert_eq!(secret_keys(&root, &section()), ["token"]);
        assert!(secret_keys(&root, &["webBoards".into(), "missing".into()]).is_empty());
    }

    #[test]
    fn edits_set_remove_and_keep_values_and_secrets() {
        let mut root = root();
        write(
            &mut root,
            &[BoardFieldEdits {
                section: section(),
                values: BTreeMap::from([
                    ("endpoint".into(), Some(json!("https://new.example"))),
                    ("limit".into(), None),
                    ("compact".into(), Some(json!(true))),
                ]),
                secrets: BTreeMap::from([
                    ("token".into(), TokenEdit::Keep),
                    ("key".into(), TokenEdit::Replace(Secret::new("k"))),
                ]),
                permissions: BTreeMap::new(),
            }],
        )
        .unwrap();
        let notes = &root["webBoards"]["notes"];
        assert_eq!(
            notes["settings"],
            json!({"endpoint": "https://new.example", "compact": true})
        );
        assert_eq!(notes["secrets"], json!({"token": "old", "key": "k"}));
        assert_eq!(notes["path"], json!("/apps/notes"));
        assert_eq!(root["github"]["token"], json!("gh"));
    }

    #[test]
    fn clearing_the_last_secret_removes_the_object() {
        let mut root = root();
        let edit = BoardFieldEdits {
            section: section(),
            secrets: BTreeMap::from([("token".into(), TokenEdit::Clear)]),
            ..Default::default()
        };
        write(&mut root, &[edit]).unwrap();
        assert!(root["webBoards"]["notes"].get("secrets").is_none());
        let missing = BoardFieldEdits {
            section: vec!["webBoards".into(), "gone".into()],
            ..Default::default()
        };
        assert!(write(&mut root, &[missing]).is_err());
    }

    #[test]
    fn a_top_level_section_is_created_but_a_missing_board_is_refused() {
        let mut root = root();
        let edit = |section: Vec<String>| BoardFieldEdits {
            section,
            values: BTreeMap::from([("roots".into(), Some(json!("key")))]),
            ..Default::default()
        };
        write(&mut root, &[edit(vec!["tsxSpaces".into()])]).unwrap();
        assert_eq!(root["tsxSpaces"]["settings"]["roots"], json!("key"));
        let missing = edit(vec!["webBoards".into(), "missing".into()]);
        assert!(write(&mut root, &[missing]).is_err());
        assert!(
            !root["webBoards"]
                .as_object()
                .unwrap()
                .contains_key("missing")
        );
    }

    #[test]
    fn blank_replacement_secrets_are_refused() {
        let edit = BoardFieldEdits {
            section: section(),
            secrets: BTreeMap::from([("token".into(), TokenEdit::Replace(Secret::new(" ")))]),
            ..Default::default()
        };
        assert!(validate(&[edit]).is_err());
        assert!(validate(&[BoardFieldEdits::default()]).is_err());
    }
}
