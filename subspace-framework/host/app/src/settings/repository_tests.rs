//! Settings › PRs favourite repositories: the draft list, Remove, Add
//! repository… through the shared search, and Save.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle, px, size};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_dashboard::repositories::RepositorySearch;

use super::{Settings, SettingsPage};

fn open(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    let path = root.join("config.json");
    std::fs::write(&path, r#"{"repos":["acme/web"],"keep":{"me":1}}"#).unwrap();
    cx.update(|cx| {
        crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
        crate::theme::initialize(cx);
        subspace_dashboard::bind_keys(cx);
        let mut board = crate::settings::navigation_tests::registration();
        board.id = "github";
        board.title = "PRs".into();
        board.settings.sections = &[];
        board.connection = subspace_dashboard::settings::ConnectionSettings::new(&[
            subspace_dashboard::settings::ConnectionField::Repositories,
        ]);
        cx.set_global(crate::registry::IncludedBoards(vec![board]));
        let config = subspace_core::config::load(&path).0;
        cx.set_global(subspace_dashboard::Services {
            credentials: Arc::new(subspace_core::credentials::Credentials::from_system(
                &config,
            )),
            config: Arc::new(config),
            tools: Default::default(),
            search: None,
            config_file: path.clone(),
        });
    });
    let file = SettingsFile::open(&path).unwrap();
    let settings =
        cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx));
    cx.simulate_window_resize(settings.into(), size(px(1100.), px(1400.)));
    settings
        .update(cx, |settings, _, cx| {
            settings.page = SettingsPage::Board("github");
            cx.notify();
        })
        .unwrap();
    settings
}

fn draw(settings: WindowHandle<Settings>, cx: &mut TestAppContext) {
    for _ in 0..3 {
        cx.update_window(settings.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        cx.run_until_parked();
    }
}

fn draft(settings: WindowHandle<Settings>, cx: &mut TestAppContext) -> Vec<String> {
    settings
        .read_with(cx, |settings, cx| {
            settings.form.repos.read(cx).favourites().to_vec()
        })
        .unwrap()
}

#[gpui_kit::test]
fn remove_edits_the_draft_and_save_applies_it_without_writing_until_then(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    let settings = open(root.path(), cx);
    draw(settings, cx);
    cx.update_window(settings.into(), |_, window, cx| {
        assert_eq!(
            window.find("settings-repositories-list").label(),
            Some("Repositories")
        );
        assert!(
            window
                .try_find("settings-repositories-add-button")
                .is_some()
        );
        window.click("settings-repository-remove:acme/web", cx);
    })
    .unwrap();
    draw(settings, cx);
    assert!(draft(settings, cx).is_empty());
    let saved = |path: &Path| SettingsFile::open(path).unwrap().values().repos.clone();
    assert_eq!(saved(&path), ["acme/web"], "Settings edits wait for Save");
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.has_unsaved_changes(cx));
            assert!(!settings.restart_required(cx), "favourites apply on Save");
            settings
                .form
                .repos
                .update(cx, |list, cx| list.toggle("acme/api", cx));
            settings.save(window, cx).unwrap();
        })
        .unwrap();
    assert_eq!(saved(&path), ["acme/api"]);
    let root_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(root_json["keep"]["me"], 1);
    cx.update(|cx| {
        assert_eq!(
            subspace_dashboard::repositories::favourites(cx),
            ["acme/api"]
        );
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[gpui_kit::test]
fn add_repository_searches_and_stars_into_the_draft(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    draw(settings, cx);
    let search: RepositorySearch =
        Arc::new(|_: &str| Ok(vec!["acme/web".to_owned(), "acme/api".to_owned()]));
    settings
        .update(cx, |settings, window, cx| {
            settings
                .form
                .repos
                .update(cx, |list, cx| list.open_search(search, window, cx));
        })
        .unwrap();
    draw(settings, cx);
    cx.update_window(settings.into(), |_, window, _| {
        assert!(window.try_find("menu").is_some());
    })
    .unwrap();
    // The favourite acme/web, then the result acme/api.
    let mut visual = gpui_kit::VisualTestContext::from_window(settings.into(), cx);
    visual.simulate_keystrokes("down enter");
    draw(settings, cx);
    draw(settings, cx);
    assert_eq!(draft(settings, cx), ["acme/web", "acme/api"]);
    cx.update_window(settings.into(), |_, window, _| {
        assert!(window.try_find("menu").is_some(), "the search stays open");
    })
    .unwrap();
    cx.simulate_keystrokes(settings.into(), "escape");
    draw(settings, cx);
    cx.update_window(settings.into(), |_, window, _| {
        assert!(window.try_find("menu").is_none());
        assert!(
            window
                .try_find("settings-repository-remove:acme/api")
                .is_some()
        );
    })
    .unwrap();
    let path = root.path().join("config.json");
    assert_eq!(
        SettingsFile::open(&path).unwrap().values().repos,
        ["acme/web"]
    );
}

#[gpui_kit::test]
fn tmp_add_button_keeps_the_page(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open(root.path(), cx);
    cx.update(|cx| {
        let mut boards = crate::registry::included_registrations(cx);
        let mut other = boards[0].clone();
        other.id = "github-copilot-agents";
        other.title = "Copilot Agents".into();
        boards.push(other);
        cx.set_global(crate::registry::IncludedBoards(boards));
    });
    settings
        .update(cx, |settings, window, cx| {
            settings.reconcile_boards(window, cx);
            settings.select_initial_board("github", cx);
        })
        .unwrap();
    draw(settings, cx);
    cx.update_window(settings.into(), |_, window, cx| {
        window.click("settings-repositories-add-button", cx);
    })
    .unwrap();
    draw(settings, cx);
    settings
        .read_with(cx, |settings, _| {
            assert_eq!(settings.page, SettingsPage::Board("github"));
        })
        .unwrap();
}
