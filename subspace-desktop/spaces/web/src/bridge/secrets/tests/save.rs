//! Saving to the config file: the file decides, as the running app serves it.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use subspace_framework::core::config::web_boards::WebBoard;
use subspace_framework::core::{config, credentials::Credentials};
use subspace_framework::dashboard::Services;

use super::super::{CHANGED, DECLARED_KEY, MAX_APP_SECRETS, TOO_MANY, app_secret_count, readable};
use super::{manifest_json, write};
use crate::bridge::Bridge;
use crate::manifest::Permission;

/// An app folder and a config file whose board `fleet` shows it, served
/// as the running app serves them.
struct Served {
    _root: tempfile::TempDir,
    path: PathBuf,
    app: PathBuf,
    bridge: Bridge,
}

impl Served {
    /// The board allows `save-secrets` and has the declared `token`.
    fn new(cx: &mut gpui_kit::App) -> Self {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(app.join("subspace-board.json"), manifest_json().to_string()).unwrap();
        let path = root.path().join("config.json");
        let entry = json!({"path": app, "secrets": {"token": "t"}});
        private_write(&path, &json!({"webBoards": {"fleet": entry}}));
        let bridge = Bridge::new(&config::load(&path).0.web_boards[0], None).unwrap();
        let served = Self {
            _root: root,
            path,
            app,
            bridge,
        };
        served.rewrite(|entry| {
            entry["permissions"] = json!({served.grant(): true});
        });
        served.serve(cx);
        served
    }

    fn grant(&self) -> String {
        self.bridge
            .manifest
            .grant_key(&self.bridge.root, Permission::SaveSecrets)
    }

    /// Edits board `fleet`'s entry in the file, as if outside the app.
    fn rewrite(&self, edit: impl FnOnce(&mut Value)) {
        let mut root = self.saved();
        edit(&mut root["webBoards"]["fleet"]);
        root["futureSetting"] = json!(42);
        private_write(&self.path, &root);
    }

    /// Runs the app with the file as it is now.
    fn serve(&self, cx: &mut gpui_kit::App) {
        let config = config::load(&self.path).0;
        cx.set_global(Services {
            credentials: Arc::new(Credentials::from_system(&config)),
            config: Arc::new(config),
            tools: Default::default(),
            search: None,
            config_file: self.path.clone(),
        });
    }

    fn saved(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(&self.path).unwrap()).unwrap()
    }

    fn save(
        &self,
        key: &str,
        value: Option<&str>,
        cx: &mut gpui_kit::App,
    ) -> Result<(), &'static str> {
        super::super::save(&self.bridge, "fleet", &write(key, value), cx)
    }
}

/// Writes a config file only its owner reads, whose secrets the app loads.
fn private_write(path: &std::path::Path, root: &Value) {
    std::fs::write(path, root.to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn running(cx: &gpui_kit::App) -> WebBoard {
    cx.global::<Services>().config.web_boards[0].clone()
}

#[gpui_kit::test]
fn a_saved_secret_reaches_the_file_and_the_running_config(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        let served = Served::new(cx);
        let owner = served.bridge.owner.clone();
        assert_eq!(served.save("fleet-a", Some("new"), cx), Ok(()));
        assert_eq!(
            readable(&served.bridge.manifest, &owner, &running(cx)),
            json!({"fleet-a": "new", "token": "t"})
        );
        let saved = served.saved();
        let key = owner.stored_key("fleet-a");
        assert_eq!(
            saved["webBoards"]["fleet"]["secrets"],
            json!({"token": "t", key.as_str(): "new"})
        );
        assert_eq!(saved["futureSetting"], 42);
        assert_eq!(served.save("fleet-a", None, cx), Ok(()));
        assert_eq!(served.save("token", None, cx), Err(DECLARED_KEY));
        assert_eq!(
            served.saved()["webBoards"]["fleet"]["secrets"],
            json!({"token": "t"})
        );
        assert!(running(cx).secrets.contains_key("token"));
        assert!(
            super::super::save(&served.bridge, "missing", &write("fleet-a", Some("t")), cx)
                .is_err()
        );
    });
}

#[gpui_kit::test]
fn a_write_never_reaches_an_entry_that_changed_in_the_file(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        let served = Served::new(cx);
        // The ID now names another app's folder; the running config does not know yet.
        let other = served.app.with_file_name("other");
        std::fs::create_dir(&other).unwrap();
        std::fs::copy(
            served.app.join("subspace-board.json"),
            other.join("subspace-board.json"),
        )
        .unwrap();
        let grant = served.grant();
        served.rewrite(|entry| entry["path"] = json!(other));
        let before = served.saved();
        assert_eq!(served.save("fleet-a", Some("t"), cx), Err(CHANGED));
        assert_eq!(served.saved(), before);

        // Back to this folder, but installed from another source now.
        served.rewrite(|entry| {
            entry["path"] = json!(served.app);
            entry["managedSource"] = json!({"kind": "folder", "original": "/apps/other",
                "content": "c"});
        });
        let before = served.saved();
        assert_eq!(served.save("fleet-a", Some("t"), cx), Err(CHANGED));
        assert_eq!(served.saved(), before);

        // No recorded source again, but the grant is off in the file.
        served.rewrite(|entry| {
            entry.as_object_mut().unwrap().remove("managedSource");
            entry["permissions"] = json!({grant: false});
        });
        let before = served.saved();
        assert_eq!(served.save("fleet-a", Some("t"), cx), Err(CHANGED));
        assert_eq!(served.saved(), before);
    });
}

#[gpui_kit::test]
fn the_limit_counts_the_secrets_in_the_file(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        let served = Served::new(cx);
        let owner = served.bridge.owner.clone();
        let full: serde_json::Map<String, Value> = (0..MAX_APP_SECRETS)
            .map(|n| (owner.stored_key(&format!("fleet-{n}")), json!("t")))
            .collect();
        served.rewrite(|entry| entry["secrets"] = Value::Object(full));
        let manifest = &served.bridge.manifest;
        assert_eq!(app_secret_count(manifest, &owner, &running(cx)), 0);
        assert_eq!(served.save("fleet-new", Some("t"), cx), Err(TOO_MANY));
        assert_eq!(served.save("fleet-0", Some("replaced"), cx), Ok(()));
    });
}

#[gpui_kit::test]
fn a_saved_secret_keeps_the_board_configuration(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        let served = Served::new(cx);
        let (full, before) = (running(cx), crate::configuration(&running(cx)));
        served.save("fleet-a", Some("t"), cx).unwrap();
        assert_ne!(running(cx), full);
        assert_eq!(crate::configuration(&running(cx)), before);
        // A secret the user changes in Settings still opens the page again.
        served.rewrite(|entry| entry["secrets"]["token"] = json!("changed"));
        served.serve(cx);
        assert_ne!(crate::configuration(&running(cx)), before);
    });
}
