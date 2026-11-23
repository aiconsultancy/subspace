//! What a configured web board shows: a web page, or a web app from a
//! folder of built files.

use std::path::Path;

use subspace_framework::core::config::web_boards::{
    WebBoard, WebBoardSource, web_board_url_allowed,
};
use subspace_framework::dashboard::settings::{BoardFields, PermissionSpec};

use crate::app::app_origin;
use crate::manifest::{self, Manifest, Permission};
use crate::page::{AppFolder, Start};
use crate::policy::location;

/// The switcher tooltip and accessible description of `board`: its title
/// and the site or app it shows.
pub fn description(board: &WebBoard) -> String {
    match &board.source {
        WebBoardSource::Url(url) => match location(url) {
            Some(site) => format!("{}: web page on {site}", board.title),
            None => format!("{}: web page", board.title),
        },
        WebBoardSource::Folder(_) => format!("{}: local web app", board.title),
    }
}

/// The settings a web app board's manifest offers, and what the manifest
/// says about the app. `None` for a web page or an app without a manifest.
pub fn settings_fields(board: &WebBoard) -> Option<BoardFields> {
    let WebBoardSource::Folder(root) = &board.source else {
        return None;
    };
    let section = vec!["webBoards".to_owned(), board.id.clone()];
    Some(match manifest::read(root) {
        Ok(None) => return None,
        Ok(Some(manifest)) => manifest_fields(section, &manifest, root),
        Err(error) => BoardFields::new(
            section,
            Vec::new(),
            vec![format!("This app cannot open. {error}")],
            Vec::new(),
        ),
    })
}

/// Board `id`'s raw entry in the config file, which records where it was
/// installed from.
pub(crate) fn entry(
    id: &str,
    cx: &gpui_kit::App,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let services = cx.try_global::<subspace_framework::dashboard::Services>()?;
    let file =
        subspace_framework::core::settings::SettingsFile::open(&services.config_file).ok()?;
    file.web_board(id)
}

/// What decides whether `board`'s page opens again when the config file
/// changes: the whole entry but the secrets its page saved itself, which
/// the page already has and reads back without a reload.
pub fn configuration(board: &WebBoard) -> WebBoard {
    let mut board = board.clone();
    board
        .secrets
        .retain(|key, _| !crate::bridge::secrets::is_app_secret(key));
    board
}

/// The manifest's fields, notes and grants. An app that may read or save
/// its own secrets also gets their count and Clear.
fn manifest_fields(section: Vec<String>, manifest: &Manifest, root: &Path) -> BoardFields {
    let permissions = manifest
        .permissions
        .iter()
        .map(|permission| {
            PermissionSpec::new(
                manifest.grant_key(root, *permission),
                format!("Allow this app to {}", permission.label()),
            )
        })
        .collect();
    let fields = BoardFields::new(
        section,
        manifest.settings.clone(),
        manifest_notes(manifest, root),
        permissions,
    );
    if manifest.permissions.contains(&Permission::Secrets)
        || manifest.permissions.contains(&Permission::SaveSecrets)
    {
        fields.with_app_secrets()
    } else {
        fields
    }
}

fn manifest_notes(manifest: &Manifest, root: &Path) -> Vec<String> {
    let asks: Vec<&str> = manifest
        .permissions
        .iter()
        .map(|permission| permission.label())
        .collect();
    let permissions = if asks.is_empty() {
        "The app asks for no permissions.".to_owned()
    } else {
        format!(
            "The app asks to {}. Permissions stay off until you allow them below and save.",
            asks.join(", ")
        )
    };
    [
        format!(
            "{} {}, from {}",
            manifest.name,
            manifest.version,
            root.display()
        ),
        permissions,
    ]
    .into_iter()
    .chain(manifest.warnings.iter().cloned())
    .collect()
}

/// The page `board` loads first, and its app folder when it has one.
pub(crate) fn start(board: &WebBoard) -> Start {
    match &board.source {
        WebBoardSource::Url(url) => Start {
            url: url.clone(),
            app: None,
        },
        WebBoardSource::Folder(root) => {
            let origin = app_origin(&board.id);
            Start {
                url: format!("{origin}/"),
                app: Some(AppFolder {
                    root: root.clone(),
                    origin,
                    bridge_enabled: manifest::read(root).is_ok_and(|manifest| manifest.is_some()),
                }),
            }
        }
    }
}

/// Why `board` cannot show its page yet, if it cannot.
pub(crate) fn problem(board: &WebBoard) -> Option<String> {
    match &board.source {
        WebBoardSource::Url(url) => url_problem(url),
        WebBoardSource::Folder(root) => folder_problem(root),
    }
}

fn url_problem(url: &str) -> Option<String> {
    let allowed = url::Url::parse(url).is_ok_and(|url| web_board_url_allowed(&url));
    (!allowed).then(|| {
        "Set an https:// address for this board under webBoards in the config file.".to_owned()
    })
}

fn folder_problem(root: &Path) -> Option<String> {
    if !root.join("index.html").is_file() {
        return Some(format!(
            "{} has no index.html. Build the app into this folder, then select Retry.",
            root.display()
        ));
    }
    manifest::read(root).err()
}

#[cfg(test)]
mod tests {
    use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

    use super::{description, problem, settings_fields, start};

    fn board(source: WebBoardSource) -> WebBoard {
        WebBoard {
            id: "notes".into(),
            title: "Notes".into(),
            source,
            settings: Default::default(),
            secrets: Default::default(),
            permissions: Default::default(),
        }
    }

    #[test]
    fn descriptions_name_the_board_and_what_it_shows() {
        let page = board(WebBoardSource::Url("https://wiki.example.com/a?b".into()));
        assert_eq!(description(&page), "Notes: web page on wiki.example.com");
        let local = board(WebBoardSource::Url("http://localhost:5173/".into()));
        assert_eq!(description(&local), "Notes: web page on localhost:5173");
        let app = board(WebBoardSource::Folder(std::env::temp_dir()));
        assert_eq!(description(&app), "Notes: local web app");
    }

    #[test]
    fn an_app_starts_at_its_private_origin() {
        let root = std::env::temp_dir();
        let start = start(&board(WebBoardSource::Folder(root.clone())));
        let app = start.app.unwrap();
        assert_eq!(start.url, format!("{}/", app.origin));
        assert_eq!(app.root, root);
        assert!(!app.bridge_enabled);
        let page = super::start(&board(WebBoardSource::Url("https://a.example/".into())));
        assert!(page.app.is_none());
    }

    #[test]
    fn an_app_folder_needs_an_index_page() {
        let root = tempfile::tempdir().unwrap();
        let app = board(WebBoardSource::Folder(root.path().to_owned()));
        assert!(problem(&app).unwrap().contains("index.html"));
        std::fs::write(root.path().join("index.html"), "<p>").unwrap();
        assert_eq!(problem(&app), None);
        assert!(problem(&board(WebBoardSource::Url("http://example.com".into()))).is_some());
        std::fs::write(root.path().join("subspace-board.json"), "{}").unwrap();
        assert!(problem(&app).unwrap().contains("subspace-board.json"));
    }

    #[test]
    fn a_manifest_gives_the_settings_page_its_fields_and_facts() {
        let root = tempfile::tempdir().unwrap();
        let app = board(WebBoardSource::Folder(root.path().to_owned()));
        assert_eq!(settings_fields(&app), None);
        let manifest = serde_json::json!({
            "name": "Notes", "version": "1.2.0",
            "permissions": ["notifications", "camera"],
            "settings": [{"key": "limit", "label": "Limit", "type": "number"}]
        });
        std::fs::write(
            root.path().join("subspace-board.json"),
            manifest.to_string(),
        )
        .unwrap();
        let fields = settings_fields(&app).unwrap();
        assert_eq!(fields.section, ["webBoards", "notes"]);
        assert_eq!(fields.fields[0].key, "limit");
        assert!(fields.notes[0].starts_with("Notes 1.2.0, from "));
        assert!(fields.notes[1].contains("show notifications"));
        assert!(fields.notes[2].contains("camera"));
        assert!(!fields.app_secrets);
        for permission in ["secrets", "save-secrets"] {
            let with_secrets = serde_json::json!({
                "name": "Notes", "version": "1.2.0", "permissions": [permission]
            });
            std::fs::write(
                root.path().join("subspace-board.json"),
                with_secrets.to_string(),
            )
            .unwrap();
            let fields = settings_fields(&app).unwrap();
            assert!(fields.app_secrets, "{permission}");
            let label = &fields.permissions[0].label;
            assert_eq!(
                label == "Allow this app to save its own secrets",
                permission == "save-secrets",
                "{label}"
            );
        }
        std::fs::write(root.path().join("subspace-board.json"), "[]").unwrap();
        let broken = settings_fields(&app).unwrap();
        assert!(broken.fields.is_empty());
        assert!(broken.notes[0].starts_with("This app cannot open."));
        assert_eq!(
            settings_fields(&board(WebBoardSource::Url("https://a.example".into()))),
            None
        );
    }
}
