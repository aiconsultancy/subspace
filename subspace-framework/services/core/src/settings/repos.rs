//! Checked immediate edits of the favourite repositories (`repos`), the one
//! list GitHub, Copilot Agents, Actions and the agent tools share.

use serde_json::Value;

use super::{SettingsError, SettingsFile};
use crate::config;

impl SettingsFile {
    /// Immediately writes `repos` as the saved list, in order. Every other
    /// value and unknown key in the file is kept, and the file stays
    /// owner-only. A file changed since it was opened is refused as stale.
    pub fn update_repos(&mut self, repos: &[String]) -> Result<(), SettingsError> {
        validate(repos)?;
        self.check_current()?;
        let mut root = self.root.clone();
        root.insert("repos".into(), values(repos));
        self.write_root(root)
    }
}

/// Each entry must be `owner/name`.
pub(super) fn validate(repos: &[String]) -> Result<(), SettingsError> {
    if repos.iter().any(|repo| !config::is_repo(repo)) {
        return Err(SettingsError::Validation {
            field: "repos",
            reason: "each entry must be owner/name",
        });
    }
    Ok(())
}

/// The saved JSON array, each name once without regard to case.
pub(super) fn values(repos: &[String]) -> Value {
    let mut unique: Vec<&String> = Vec::new();
    for repo in repos {
        if !unique.iter().any(|known| known.eq_ignore_ascii_case(repo)) {
            unique.push(repo);
        }
    }
    unique
        .into_iter()
        .cloned()
        .map(Value::String)
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
mod tests {
    use crate::settings::{SettingsError, SettingsFile};
    use serde_json::{Value, json};
    use std::fs;

    fn saved(path: &std::path::Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn favourites_save_alone_and_keep_every_other_value() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(
            &path,
            r#"{"repos":["acme/web"],"github":{"token":"gh"},"unknown":{"keep":7}}"#,
        )
        .unwrap();
        let mut file = SettingsFile::open(&path).unwrap();
        let list = ["acme/web".to_owned(), "acme/api".into(), "ACME/API".into()];
        file.update_repos(&list).unwrap();
        let root = saved(&path);
        assert_eq!(root["repos"], json!(["acme/web", "acme/api"]));
        assert_eq!(root["github"]["token"], "gh");
        assert_eq!(root["unknown"]["keep"], 7);
        assert!(root.get("mcp").is_none(), "nothing else is written");
        assert_eq!(file.values().repos, ["acme/web", "acme/api"]);
        file.update_repos(&[]).unwrap();
        assert_eq!(saved(&path)["repos"], json!([]));
        assert_eq!(
            crate::config::load(&path).0.github.token.unwrap().expose(),
            "gh"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_new_or_shared_file_becomes_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        for existing in [false, true] {
            let path = directory.path().join(format!("{existing}.json"));
            if existing {
                fs::write(&path, "{}").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            }
            let mut file = SettingsFile::open(&path).unwrap();
            file.update_repos(&["acme/web".to_owned()]).unwrap();
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "existing: {existing}");
        }
    }

    #[test]
    fn invalid_names_and_stale_files_write_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, r#"{"repos":["acme/web"]}"#).unwrap();
        let mut file = SettingsFile::open(&path).unwrap();
        assert!(matches!(
            file.update_repos(&["not a repo".to_owned()]),
            Err(SettingsError::Validation { field: "repos", .. })
        ));
        fs::write(&path, r#"{"external":true}"#).unwrap();
        assert!(matches!(
            file.update_repos(&["acme/api".to_owned()]),
            Err(SettingsError::Stale)
        ));
        assert_eq!(saved(&path), json!({"external": true}));
    }
}
