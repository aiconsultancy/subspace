//! The web Spaces Subspace ships: Fleet Console, from
//! `web-spaces/fleet-console`, whose package files the build script embeds.
//! Each starts in the tab strip without an install, as a folder web board
//! the app keeps up to date (see `subspace_web_board::builtin`).

use subspace_framework::product::ProductDescriptor;
use subspace_web_board::builtin::BuiltIn;

include!(concat!(env!("OUT_DIR"), "/web_spaces.rs"));

/// Fleet Console, under the board ID the installers and docs name.
pub(crate) const FLEET_CONSOLE: BuiltIn = BuiltIn {
    id: "fleet-console",
    title: "Fleet Console",
    files: FLEET_CONSOLE_FILES,
};

/// The folder beside the config file that holds the built-in Spaces' files,
/// with the web Spaces users install.
const STORE: &str = "web-board-apps";

/// Adds the built-in web Spaces to the config file before the desktop reads
/// it. Web Spaces need a native web view, which Linux builds do not have.
/// A failure is reported and the app starts without them.
pub(crate) fn install(descriptor: &ProductDescriptor, version: &str) {
    if !cfg!(any(target_os = "macos", windows)) {
        return;
    }
    let config = match subspace_framework::shell::prepare_context(descriptor.clone(), version) {
        Ok(context) => subspace_framework::shell::config_file(&context),
        Err(error) => {
            eprintln!("Built-in web Spaces were not added: {error}");
            return;
        }
    };
    let Some(store) = config.parent().map(|folder| folder.join(STORE)) else {
        return;
    };
    if let Err(error) = subspace_web_board::builtin::install(&FLEET_CONSOLE, &config, &store) {
        eprintln!("{} was not added: {error}", FLEET_CONSOLE.title);
    }
}

#[cfg(test)]
mod tests {
    use super::FLEET_CONSOLE;

    #[test]
    fn fleet_console_ships_its_package_files_only() {
        let names: Vec<&str> = FLEET_CONSOLE.files.iter().map(|(name, _)| *name).collect();
        for required in [
            "index.html",
            "subspace-board.json",
            "styles.css",
            "src/main.js",
        ] {
            assert!(names.contains(&required), "{required}");
        }
        assert!(
            names
                .iter()
                .all(|name| !name.starts_with("test/") && *name != "mise.toml"),
            "{names:?}"
        );
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert!(
            FLEET_CONSOLE
                .files
                .iter()
                .all(|(_, bytes)| !bytes.is_empty())
        );
    }

    #[test]
    fn the_package_rules_match_the_package_script() {
        use super::super::web_spaces_build::packaged;
        for (name, is_dir) in [("src", true), ("index.html", false), ("README.md", false)] {
            assert!(packaged(name, is_dir), "{name}");
        }
        for (name, is_dir) in [
            ("test", true),
            ("node_modules", true),
            (".git", true),
            ("mise.toml", false),
            ("package.json", false),
            ("reducer.test.js", false),
            (".DS_Store", false),
        ] {
            assert!(!packaged(name, is_dir), "{name}");
        }
    }
}
