//! A new install opens in [`NEW_INSTALL_THEME`]: the desktop app's first
//! start, with neither a config file nor a state file, records it as the
//! saved theme. An existing install keeps the theme it shows.

use subspace_descriptor::NEW_INSTALL_THEME;
use subspace_framework::product::ProductDescriptor;

/// Records the new-install theme when this is a new install. Call it for the
/// desktop app only, before anything writes the config file. A build with
/// its own default theme (`SUBSPACE_DEFAULT_THEME`) records none, so every
/// install of it shows that default. A failure is reported and the app
/// starts with its default theme.
pub(crate) fn record_theme(descriptor: &ProductDescriptor, version: &str) {
    let recorded = theme_for(option_env!("SUBSPACE_DEFAULT_THEME"))
        .map(|theme| record(descriptor, version, theme))
        .unwrap_or(Ok(false));
    report(recorded);
}

/// The theme a new install records: none when the build sets its own default.
fn theme_for(build_default: Option<&str>) -> Option<&'static str> {
    build_default.is_none().then_some(NEW_INSTALL_THEME)
}

fn record(descriptor: &ProductDescriptor, version: &str, theme: &str) -> Result<bool, String> {
    subspace_framework::shell::prepare_context(descriptor.clone(), version).and_then(|context| {
        subspace_framework::shell::record_new_install_theme(&context, theme, &[])
    })
}

fn report(recorded: Result<bool, String>) {
    if let Err(error) = recorded {
        eprintln!("The theme for a new install was not recorded: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use subspace_descriptor::{NEW_INSTALL_THEME, subspace_descriptor};
    use subspace_framework::core::state::State;
    use subspace_framework::product::{OsKind, PlatformEnv, ProductContext, validate_product};
    use subspace_framework::shell::record_new_install_theme;

    use super::theme_for;

    fn context(root: &Path) -> ProductContext {
        let product = validate_product(&subspace_descriptor()).unwrap();
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

    fn state_file(root: &Path) -> std::path::PathBuf {
        root.join("state")
            .join(subspace_framework::product::product_data_name())
            .join("state.json")
    }

    #[test]
    fn a_new_subspace_install_records_twilight_two() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            record_new_install_theme(&context(root.path()), NEW_INSTALL_THEME, &[]),
            Ok(true)
        );
        let state = State::load(&state_file(root.path()));
        assert_eq!(state.get("appearance.theme"), Some("Twilight Two"));
    }

    #[test]
    fn an_existing_subspace_install_without_a_saved_theme_records_nothing() {
        let root = tempfile::tempdir().unwrap();
        let file = state_file(root.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{}").unwrap();
        assert_eq!(
            record_new_install_theme(&context(root.path()), NEW_INSTALL_THEME, &[]),
            Ok(false)
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
    }

    #[test]
    fn a_build_with_its_own_default_theme_records_none() {
        assert_eq!(theme_for(None), Some(NEW_INSTALL_THEME));
        assert_eq!(theme_for(Some("Solarized Dark")), None);
    }
}
