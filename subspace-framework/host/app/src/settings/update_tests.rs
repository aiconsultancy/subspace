use std::path::Path;
use std::sync::Arc;

use gpui_kit::{TestAppContext, WindowHandle};
use subspace_core::credentials::GitHubAccounts;
use subspace_core::settings::SettingsFile;
use subspace_core::state::State;
use subspace_dashboard::{CurrentProduct, prefs};
use subspace_product::{
    BoardDescriptor, OsKind, PlatformEnv, ProductContext, ProductDescriptor, validate_product,
};

use super::Settings;
use crate::theme::{self, THEME_PREF};

fn open_settings(root: &Path, cx: &mut TestAppContext) -> WindowHandle<Settings> {
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root, root.join("state.json"));
        prefs::set(cx, THEME_PREF, "Solarized Light");
        theme::initialize(cx);
        prefs::save(cx).unwrap();
    });
    let path = root.join("config.json");
    std::fs::write(&path, "{}").unwrap();
    let file = SettingsFile::open(&path).unwrap();
    cx.add_window(move |window, cx| Settings::new(file, GitHubAccounts::default(), window, cx))
}

/// A product with no update source, for the "hidden without one" test.
fn product_without_update(root: &Path) -> ProductContext {
    let descriptor = ProductDescriptor {
        id: "subspace".into(),
        binary_name: "subspace".into(),
        display_name: "Subspace".into(),
        bundle_id: "com.example.subspace".into(),
        release_repository: "example-org/example-app".into(),
        icon_paths: vec!["assets/subspace.icns".into()],
        credits: Vec::new(),
        themes: vec!["one-dark".into()],
        default_theme: Some("Subspace One Dark".into()),
        boards: vec![BoardDescriptor {
            id: "github".into(),
            aliases: Vec::new(),
            former_ids: Vec::new(),
            default_enabled: true,
        }],
        update: None,
        command_mount: Default::default(),
    };
    let product = validate_product(&descriptor).unwrap();
    let env = PlatformEnv {
        os: OsKind::Linux,
        home: root.join("home"),
        xdg_config_home: Some(root.join("config")),
        xdg_state_home: Some(root.join("state")),
        app_data: None,
        local_app_data: None,
    };
    ProductContext::new(product, env)
}

#[gpui_kit::test]
fn check_updates_defaults_on_and_the_saved_choice_is_read_back(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let settings = open_settings(root.path(), cx);
    settings
        .update(cx, |settings, window, cx| {
            assert!(settings.check_updates());
            assert!(super::has_update_source(cx));
            settings.set_check_updates(false, cx);
            settings.save(window, cx).unwrap();
            prefs::save(cx).unwrap();
        })
        .unwrap();
    let state = State::load(&root.path().join("state.json"));
    assert_eq!(state.get(super::CHECK_UPDATES_KEY), Some("0"));
}

#[gpui_kit::test]
fn the_update_switch_is_hidden_without_an_update_source(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
        cx.set_global(CurrentProduct(Arc::new(product_without_update(
            root.path(),
        ))));
        theme::initialize(cx);
        assert!(!super::has_update_source(cx));
    });
}
