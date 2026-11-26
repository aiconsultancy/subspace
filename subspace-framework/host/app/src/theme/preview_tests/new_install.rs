//! A new install's recorded theme: only a start with neither product file
//! records it, and an existing install shows what it showed before.

use std::fs;
use std::path::Path;

use gpui_kit::TestAppContext;
use gpui_kit::component::{Theme, ThemeMode};
use subspace_core::state::State;
use subspace_dashboard::prefs;

use super::{init_app, product_context};
use crate::product::ProductFiles;
use crate::theme::{THEME_PREF, initialize, record_new_install_theme, selected_theme};

/// A dark-only bundled variant, as a product would choose for new installs.
const NEW_INSTALL: &str = "Twilight Two";
/// The fixture product's default, which an install without a saved theme shows.
const DEFAULT: &str = "Cupertino Dark";

fn product_files(root: &Path) -> ProductFiles {
    ProductFiles::for_context(&product_context(root))
}

fn record(root: &Path, theme: &str) -> Result<bool, String> {
    record_new_install_theme(&product_context(root), theme, &[])
}

fn write(file: &Path, text: &str) {
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}

fn contents(file: &Path) -> Option<String> {
    fs::read_to_string(file).ok()
}

#[test]
fn a_start_without_a_config_or_state_file_records_the_theme() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(record(root.path(), NEW_INSTALL), Ok(true));
    let files = product_files(root.path());
    assert_eq!(State::load(&files.state).get(THEME_PREF), Some(NEW_INSTALL));
    assert!(!files.config.exists(), "the config file is not created");
    // The second start finds the state file and changes nothing.
    let saved = contents(&files.state);
    assert_eq!(record(root.path(), NEW_INSTALL), Ok(false));
    assert_eq!(contents(&files.state), saved);
}

/// A saved theme, no saved theme, a malformed or blank state file, and a
/// config file with no state file, malformed or not: none is a new install,
/// and neither file is written or created.
#[test]
fn an_existing_install_is_left_alone_whatever_its_files_hold() {
    let cases = [
        (None, Some(r#"{"appearance.theme":"Solarized Light"}"#)),
        (None, Some(r#"{"app.dashboard":"github"}"#)),
        (None, Some("{malformed")),
        (None, Some("")),
        (Some(r#"{"repos":["acme/web"]}"#), None),
        (Some("{malformed"), None),
        (Some("{}"), Some("{}")),
    ];
    for (config, state) in cases {
        let root = tempfile::tempdir().unwrap();
        let files = product_files(root.path());
        for (file, text) in [(&files.config, config), (&files.state, state)] {
            if let Some(text) = text {
                write(file, text);
            }
        }
        assert_eq!(
            record(root.path(), NEW_INSTALL),
            Ok(false),
            "{config:?} {state:?}"
        );
        assert_eq!(contents(&files.config).as_deref(), config);
        assert_eq!(contents(&files.state).as_deref(), state);
    }
}

#[cfg(unix)]
#[test]
fn an_unreadable_or_linked_state_file_is_an_existing_install() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let root = tempfile::tempdir().unwrap();
    let files = product_files(root.path());
    write(&files.state, r#"{"appearance.theme":"Solarized Light"}"#);
    fs::set_permissions(&files.state, fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(record(root.path(), NEW_INSTALL), Ok(false));
    fs::set_permissions(&files.state, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        State::load(&files.state).get(THEME_PREF),
        Some("Solarized Light")
    );

    let root = tempfile::tempdir().unwrap();
    let files = product_files(root.path());
    fs::create_dir_all(files.state.parent().unwrap()).unwrap();
    symlink(root.path().join("missing.json"), &files.state).unwrap();
    assert_eq!(record(root.path(), NEW_INSTALL), Ok(false));
    assert!(fs::symlink_metadata(&files.state).unwrap().is_symlink());
}

/// When the state folder cannot be searched, a new install cannot be told
/// from an existing one: nothing is written and the reason is returned.
#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_read_records_nothing() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().unwrap();
    let files = product_files(root.path());
    let folder = files.state.parent().unwrap();
    fs::create_dir_all(folder).unwrap();
    fs::set_permissions(folder, fs::Permissions::from_mode(0o000)).unwrap();
    let result = record(root.path(), NEW_INSTALL);
    fs::set_permissions(folder, fs::Permissions::from_mode(0o700)).unwrap();
    let error = result.unwrap_err();
    assert!(error.contains("state.json"), "{error}");
    assert!(!files.state.exists());
}

#[test]
fn a_theme_outside_the_catalogue_is_refused_and_nothing_is_written() {
    let root = tempfile::tempdir().unwrap();
    let error = record(root.path(), "Missing Theme").unwrap_err();
    assert!(error.contains("new install theme"), "{error}");
    assert!(error.contains("Missing Theme"), "{error}");
    assert!(!product_files(root.path()).state.exists());
}

/// Product documents that startup refuses record nothing, even when the
/// theme itself is in the catalogue, so a corrected build still records it.
#[test]
fn product_documents_startup_refuses_record_nothing() {
    let root = tempfile::tempdir().unwrap();
    let reused = crate::theme::THEME_JSON.replace(crate::theme::THEME_NAME, "cupertino dark");
    let reused: &'static str = Box::leak(reused.into_boxed_str());
    let documents: crate::ProductThemes = Box::leak(Box::new([("example-dark", reused)]));
    let context = product_context(root.path());
    let error = record_new_install_theme(&context, NEW_INSTALL, documents).unwrap_err();
    assert!(error.contains("example-dark"), "{error}");
    assert!(!product_files(root.path()).state.exists());
    assert_eq!(record(root.path(), NEW_INSTALL), Ok(true));
}

/// The app never follows the system appearance: a new install on a
/// light-mode system opens in the recorded dark variant, in dark mode.
#[gpui_kit::test]
fn a_new_install_on_a_light_mode_system_opens_in_the_dark_theme(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(record(root.path(), NEW_INSTALL), Ok(true));
    let state = product_files(root.path()).state;
    cx.update(|cx| {
        init_app(cx, root.path(), state);
        Theme::change(ThemeMode::Light, None, cx);
        initialize(cx);
        assert_eq!(selected_theme(cx), NEW_INSTALL);
        let theme = Theme::global(cx);
        assert_eq!(theme.theme_name().as_ref(), NEW_INSTALL);
        assert!(theme.is_dark());
    });
}

/// An existing install whose state file has no saved theme keeps showing
/// the product's default, and still has no saved theme.
#[gpui_kit::test]
fn an_existing_install_without_a_saved_theme_keeps_the_default(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let state = product_files(root.path()).state;
    write(&state, r#"{"app.dashboard":"github"}"#);
    assert_eq!(record(root.path(), NEW_INSTALL), Ok(false));
    cx.update(|cx| {
        init_app(cx, root.path(), state);
        initialize(cx);
        assert_eq!(selected_theme(cx), DEFAULT);
        assert_eq!(Theme::global(cx).theme_name().as_ref(), DEFAULT);
        assert_eq!(prefs::get(cx, THEME_PREF), None);
    });
}
