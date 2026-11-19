//! The section cache on disk: Subspace keeps every account in one file,
//! `subspace/github-cache.json`, beside the state file. Lists show from it
//! at once and then refresh (G-02).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{SectionData, parse_account_cache};

pub type AccountCache = HashMap<&'static str, SectionData>;

/// `github-cache.json` in the state file's directory.
pub fn path() -> Option<PathBuf> {
    subspace_framework::core::state::sibling("github-cache.json")
}

/// Every account's stored sections, keyed by the lowercased login. A missing
/// or broken file reads as empty.
pub fn load(file: &Path) -> HashMap<String, AccountCache> {
    let Some(value) = fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return HashMap::new();
    };
    let Some(accounts) = value.as_object() else {
        return HashMap::new();
    };
    accounts
        .iter()
        .map(|(login, sections)| (login.to_lowercase(), parse_account_cache(sections)))
        .collect()
}

/// Writes the file through a temporary file and a rename.
pub fn save(file: &Path, accounts: &HashMap<String, AccountCache>) -> std::io::Result<()> {
    let text = serde_json::to_string(accounts)?;
    subspace_framework::core::state::write_atomic(file, &text)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{load, save};
    use crate::model::SectionData;
    use crate::sample;

    #[test]
    fn accounts_survive_a_save_and_load() {
        let dir = std::env::temp_dir().join(format!("subspace-gh-cache-{}", std::process::id()));
        let file = dir.join("github-cache.json");
        let mut sections = HashMap::new();
        sections.insert(
            "mine",
            SectionData {
                total: 7,
                items: sample::items(),
                at: 42,
            },
        );
        let mut accounts = HashMap::new();
        accounts.insert("ann".to_string(), sections);
        save(&file, &accounts).unwrap();
        let loaded = load(&file);
        assert_eq!(loaded["ann"]["mine"].total, 7);
        assert_eq!(loaded["ann"]["mine"].items, sample::items());
        assert!(load(&dir.join("missing.json")).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
