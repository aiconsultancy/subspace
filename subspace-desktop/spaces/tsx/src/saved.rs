//! The Space's saved choices, in its own config section: the trusted
//! signing roots under `settings`, and the capability
//! grants under `permissions`. Settings edits them; the Space only reads.

use std::path::Path;

use serde_json::{Map, Value};

/// The config file section: `{"tsxSpaces": {"settings": …, "permissions": …}}`.
pub(crate) const SECTION: &str = "tsxSpaces";
/// The trusted root public keys, base64, separated by commas or spaces.
pub(crate) const TRUSTED_ROOTS: &str = "trustedRoots";

/// What Settings saved for TSX Spaces.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Saved {
    pub(crate) roots: Vec<String>,
    pub(crate) permissions: Map<String, Value>,
}

impl Saved {
    /// Whether the grant `key` is on.
    pub(crate) fn allows(&self, key: &str) -> bool {
        self.permissions.get(key).and_then(Value::as_bool) == Some(true)
    }
}

/// Reads the section from the config file. A missing or unreadable file
/// saves nothing: no roots, no grants.
pub(crate) fn load(config_file: &Path) -> Saved {
    std::fs::read_to_string(config_file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|root| parse(&root))
        .unwrap_or_default()
}

/// The section's values in the config file's JSON.
pub(crate) fn parse(root: &Value) -> Saved {
    let section = &root[SECTION];
    let settings = &section["settings"];
    Saved {
        roots: split_roots(settings[TRUSTED_ROOTS].as_str().unwrap_or_default()),
        permissions: section["permissions"]
            .as_object()
            .cloned()
            .unwrap_or_default(),
    }
}

/// The keys in a roots field, in the order given, each once.
pub(crate) fn split_roots(text: &str) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    for key in text
        .split([',', ' ', '\n', '\t'])
        .filter(|key| !key.is_empty())
    {
        if !roots.iter().any(|root| root == key) {
            roots.push(key.to_owned());
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::{parse, split_roots};
    use serde_json::json;

    #[test]
    fn nothing_saved_trusts_no_root() {
        let saved = parse(&json!({}));
        assert!(saved.roots.is_empty());
        assert!(!saved.allows("anything"));
    }

    #[test]
    fn reads_roots_and_grants() {
        let saved = parse(&json!({"tsxSpaces": {
            "settings": {"trustedRoots": "AAA, BBB\nAAA"},
            "permissions": {"on": true, "off": false, "text": "true"}
        }}));
        assert_eq!(saved.roots, ["AAA", "BBB"]);
        assert!(saved.allows("on"));
        assert!(!saved.allows("off"));
        assert!(!saved.allows("text"));
    }

    #[test]
    fn splits_roots_on_commas_and_white_space() {
        assert_eq!(split_roots(" a,b  c\td\n"), ["a", "b", "c", "d"]);
    }
}
