//! Typed setting declarations in a web board manifest.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use subspace_framework::dashboard::settings::{FieldKind, FieldSpec};

use super::{MAX_NAME_CHARS, array, bounded};

const MAX_HELP_CHARS: usize = 200;
const MAX_FIELDS: usize = 32;
const MAX_KEY_CHARS: usize = 40;
const MAX_OPTIONS: usize = 50;

pub(super) fn parse(value: Option<&Value>) -> Result<Vec<FieldSpec>, String> {
    let entries = array(value, "settings")?;
    if entries.len() > MAX_FIELDS {
        return Err(format!("expected at most {MAX_FIELDS} settings."));
    }
    let mut keys = BTreeSet::new();
    let mut specs = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let spec = field(entry).map_err(|error| format!("settings[{index}]: {error}"))?;
        if !keys.insert(spec.key.clone()) {
            return Err(format!(
                "settings[{index}]: the key {} is used twice.",
                spec.key
            ));
        }
        specs.push(spec);
    }
    Ok(specs)
}

fn field(entry: &Value) -> Result<FieldSpec, String> {
    let entry = entry.as_object().ok_or("expected an object.")?;
    let key = entry.get("key").and_then(Value::as_str).unwrap_or_default();
    if !valid_key(key) {
        return Err(format!(
            "expected key to be 1 to {MAX_KEY_CHARS} letters, digits, _ or -, starting with a letter."
        ));
    }
    let help = match entry.get("help") {
        None | Some(Value::Null) => None,
        Some(_) => Some(bounded(entry, "help", MAX_HELP_CHARS)?),
    };
    Ok(FieldSpec::new(
        key.to_owned(),
        bounded(entry, "label", MAX_NAME_CHARS)?,
        help,
        kind(entry)?,
    ))
}

fn valid_key(key: &str) -> bool {
    key.len() <= MAX_KEY_CHARS
        && key.starts_with(|c: char| c.is_ascii_alphabetic())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn kind(entry: &Map<String, Value>) -> Result<FieldKind, String> {
    let default = entry.get("default").filter(|value| !value.is_null());
    let text_default = || match default {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err("expected default to be text.".to_owned()),
    };
    match entry.get("type").and_then(Value::as_str) {
        Some("text") => Ok(FieldKind::Text {
            default: text_default()?,
        }),
        Some("url") => Ok(FieldKind::Url {
            default: text_default()?,
        }),
        Some("number") => match default {
            None => Ok(FieldKind::Number { default: None }),
            Some(value) => value
                .as_f64()
                .map(|number| FieldKind::Number {
                    default: Some(number),
                })
                .ok_or_else(|| "expected default to be a number.".into()),
        },
        Some("boolean") => match default {
            None => Ok(FieldKind::Boolean { default: false }),
            Some(Value::Bool(on)) => Ok(FieldKind::Boolean { default: *on }),
            Some(_) => Err("expected default to be true or false.".into()),
        },
        Some("select") => select(entry, text_default()?),
        Some("secret") if default.is_none() => Ok(FieldKind::Secret),
        Some("secret") => Err("a secret cannot have a default.".into()),
        _ => Err("expected type to be text, url, number, boolean, select or secret.".into()),
    }
}

fn select(entry: &Map<String, Value>, default: Option<String>) -> Result<FieldKind, String> {
    let options: Vec<String> = array(entry.get("options"), "options")?
        .iter()
        .map(|option| option.as_str().map(str::to_owned))
        .collect::<Option<_>>()
        .ok_or("expected options to be a list of text.")?;
    let usable = !options.is_empty()
        && options.len() <= MAX_OPTIONS
        && options
            .iter()
            .all(|option| !option.is_empty() && option.chars().count() <= MAX_NAME_CHARS);
    if !usable {
        return Err(format!(
            "expected 1 to {MAX_OPTIONS} options of 1 to {MAX_NAME_CHARS} characters."
        ));
    }
    if default
        .as_ref()
        .is_some_and(|default| !options.contains(default))
    {
        return Err("expected default to be one of the options.".into());
    }
    Ok(FieldKind::Select { options, default })
}
