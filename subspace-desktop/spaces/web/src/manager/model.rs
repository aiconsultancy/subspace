//! Form validation and native entry edits, independent of rendering.

use serde_json::{Map, Value};
use subspace_framework::core::config::web_boards::{MAX_WEB_BOARD_TITLE_CHARS, valid_web_board_id};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SourceKind {
    #[default]
    Website,
    Folder,
    Zip,
    Repository,
    Release,
}

impl SourceKind {
    pub(super) const ALL: [Self; 5] = [
        Self::Website,
        Self::Folder,
        Self::Zip,
        Self::Repository,
        Self::Release,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Website => "Website URL",
            Self::Folder => "Local folder",
            Self::Zip => "Local ZIP",
            Self::Repository => "GitHub repository",
            Self::Release => "GitHub release ZIP",
        }
    }

    pub(super) fn is_github(self) -> bool {
        matches!(self, Self::Repository | Self::Release)
    }

    pub(super) fn from_label(label: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.label() == label)
            .unwrap_or_default()
    }
}

pub(super) fn validate_identity(id: &str, title: &str) -> Result<(), String> {
    if !valid_web_board_id(id) {
        return Err("Use a board ID starting with a lower-case letter, followed by letters, digits or hyphens, up to 40 characters.".into());
    }
    validate_title(title)
}

pub(super) fn validate_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() || title.chars().count() > MAX_WEB_BOARD_TITLE_CHARS {
        return Err("Enter a board name of 1 to 60 characters.".into());
    }
    Ok(())
}

pub(super) fn named_entry(mut entry: Map<String, Value>, title: &str) -> Map<String, Value> {
    entry.insert("title".into(), Value::String(title.trim().to_owned()));
    entry
}

/// Binds the page secrets in `entry` that have no source owner yet to the
/// app it shows now, before it takes `next`: see
/// [`crate::bridge::secrets::owner::bind_unowned`]. Reads the manifests of
/// both apps; a website has none.
pub(super) fn bind_page_secrets(
    entry: &mut Map<String, Value>,
    next: &subspace_framework::core::config::web_boards::WebBoardSource,
) {
    use subspace_framework::core::config::web_boards::WebBoardSource;
    let manifest = |path: &std::path::Path| crate::manifest::read(path).ok().flatten();
    let declared: Vec<String> = match next {
        WebBoardSource::Folder(path) => manifest(path)
            .map(|next| next.settings.into_iter().map(|field| field.key).collect())
            .unwrap_or_default(),
        WebBoardSource::Url(_) => Vec::new(),
    };
    let identity = entry
        .get("path")
        .and_then(Value::as_str)
        .map(std::path::Path::new)
        .and_then(|path| Some(manifest(path)?.identity(path)));
    crate::bridge::secrets::owner::bind_unowned(entry, &declared, identity.as_deref());
}

pub(super) fn install_entry(
    mut entry: Map<String, Value>,
    source: &subspace_framework::core::config::web_boards::WebBoardSource,
    metadata: Value,
) -> Map<String, Value> {
    use subspace_framework::core::config::web_boards::WebBoardSource;
    entry.remove("url");
    entry.remove("path");
    match source {
        WebBoardSource::Url(url) => entry.insert("url".into(), url.clone().into()),
        WebBoardSource::Folder(path) => {
            entry.insert("path".into(), path.to_string_lossy().into_owned().into())
        }
    };
    entry.insert("managedSource".into(), metadata);
    entry
}

#[derive(Default)]
pub(super) struct Epoch {
    revision: u64,
    account: Option<String>,
}

impl Epoch {
    pub(super) fn new(account: Option<String>) -> Self {
        Self {
            revision: 0,
            account: account.map(|login| login.to_ascii_lowercase()),
        }
    }

    pub(super) fn sync(&mut self, account: Option<String>) -> bool {
        let account = account.map(|login| login.to_ascii_lowercase());
        if self.account == account {
            return false;
        }
        self.account = account;
        self.invalidate();
        true
    }

    pub(super) fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn ticket(&mut self) -> u64 {
        self.invalidate();
        self.revision
    }

    pub(super) fn accepts(&self, ticket: u64) -> bool {
        self.revision == ticket
    }
}

#[cfg(test)]
mod tests {
    use super::{Epoch, install_entry, named_entry, validate_identity};
    use serde_json::json;
    use subspace_framework::core::config::web_boards::WebBoardSource;

    #[test]
    fn switching_away_and_back_does_not_revalidate_old_operations() {
        let mut epoch = Epoch::new(Some("test-first".into()));
        let ticket = epoch.ticket();
        assert!(epoch.accepts(ticket));
        assert!(epoch.sync(Some("test-second".into())));
        assert!(epoch.sync(Some("test-first".into())));
        assert!(!epoch.accepts(ticket));
        let current = epoch.ticket();
        assert!(!epoch.sync(Some("test-first".into())));
        assert!(epoch.accepts(current));
        epoch.invalidate();
        assert!(!epoch.accepts(current));
    }

    #[test]
    fn source_replacement_preserves_private_and_future_fields() {
        let entry = json!({"path":"/old", "settings":{"units":"metric"},
            "secrets":{"test-only-key":"non-working-test-value"},
            "permissions":{"old-fingerprint.badge":true}, "future":42});
        let next = install_entry(
            entry.as_object().unwrap().clone(),
            &WebBoardSource::Url("https://example.com/".into()),
            json!({"kind":"website"}),
        );
        let next = named_entry(next, " New name ");
        assert!(!next.contains_key("path"));
        assert_eq!(next["url"], "https://example.com/");
        assert_eq!(next["title"], "New name");
        assert_eq!(next["settings"]["units"], "metric");
        assert_eq!(next["secrets"], entry["secrets"]);
        assert_eq!(next["permissions"], entry["permissions"]);
        assert_eq!(next["future"], 42);
    }

    #[test]
    fn an_update_binds_page_secrets_to_its_source_and_keeps_new_declared_keys() {
        use crate::bridge::secrets::Owner;
        use crate::install::{ManagedSource, RepositoryRef};
        let root = tempfile::tempdir().unwrap();
        let app = |name: &str, settings: serde_json::Value| {
            let folder = root.path().join(name);
            std::fs::create_dir(&folder).unwrap();
            let manifest = json!({"name": "Fleet", "version": name, "settings": settings});
            std::fs::write(folder.join("subspace-board.json"), manifest.to_string()).unwrap();
            let read = crate::manifest::read(&folder).unwrap().unwrap();
            (read.identity(&folder), folder)
        };
        let (old_identity, old) = app("1", json!([]));
        let token = json!([{"key": "token", "label": "Token", "type": "secret"}]);
        let (new_identity, new) = app("2", token);
        let installed = |commit: &str| ManagedSource::Repository {
            repository: "org/fleet".into(),
            reference: RepositoryRef::DefaultBranch,
            board_path: "dist".into(),
            commit: commit.into(),
            content: commit.into(),
        };
        let mut entry = json!({"path": old, "secrets": {"fleet-a": "ownerless", "token": "t",
            (format!("{}:fleet-b", &old_identity[..32])): "folder and manifest"},
            "managedSource": serde_json::to_value(installed("c1")).unwrap()})
        .as_object()
        .unwrap()
        .clone();
        let next = WebBoardSource::Folder(new.clone());
        super::bind_page_secrets(&mut entry, &next);
        let owner = Owner::new(&new_identity, &next, Some(&installed("c2")));
        let secrets = entry["secrets"].as_object().unwrap();
        assert_eq!(secrets["token"], "t", "the new app declares it");
        assert_eq!(secrets[&owner.stored_key("fleet-a")], "ownerless");
        assert_eq!(secrets[&owner.stored_key("fleet-b")], "folder and manifest");
        assert_eq!(secrets.len(), 3);
    }

    #[test]
    fn identity_validation_does_not_accept_colliding_or_empty_names() {
        assert!(validate_identity("weather", "Weather").is_ok());
        assert!(validate_identity("../weather", "Weather").is_err());
        assert!(validate_identity("weather", " ").is_err());
        assert!(validate_identity("weather", &"x".repeat(61)).is_err());
    }
}
