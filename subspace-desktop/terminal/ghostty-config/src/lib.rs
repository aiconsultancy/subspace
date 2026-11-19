//! Zig-free parser for the Ghostty configuration and theme files the
//! terminal panel reads (REQ-29 to REQ-33). This crate depends on neither
//! `gpui-kit` nor `libghostty-vt`, so it builds without GPUI or Zig.
//!
//! # Loading a configuration
//!
//! ```no_run
//! use subspace_ghostty_config::{Paths, load};
//! use std::path::PathBuf;
//!
//! let paths = Paths {
//!     xdg_config_home: PathBuf::from("/home/fixture/.config"),
//!     macos_app_support: None,
//!     subspace_layer: PathBuf::from("/home/fixture/.config/subspace/ghostty"),
//! };
//! let (config, warnings) = load(&paths);
//! for warning in &warnings {
//!     println!("{}:{}: {}", warning.file.display(), warning.line, warning.reason);
//! }
//! println!("{:?}", config.background);
//! ```
//!
//! [`load`] reads the files in REQ-29 order (XDG config directory, then the
//! macOS Application Support pair, then the Subspace layer), follows
//! `config-file` includes, and applies only the REQ-31 supported keys.
//! [`theme::resolve_theme`] (task 05) separately resolves the `theme` value
//! this returns into colour overrides.
//!
//! # Supported keys (REQ-31)
//!
//! `background`, `foreground`, `palette`, `cursor-color`, `cursor-text`,
//! `cursor-style`, `cursor-style-blink`, `selection-background`,
//! `selection-foreground`, `font-family` and its bold, italic and
//! bold-italic forms, `font-size`, `adjust-cell-height`,
//! `scrollback-limit`, `term`, `command`, `working-directory`,
//! `clipboard-read`, `clipboard-write`, `clipboard-paste-protection`,
//! `clipboard-paste-bracketed-safe`, `copy-on-select`,
//! `confirm-close-surface`, `mouse-scroll-multiplier`,
//! `window-padding-x`, `window-padding-y`, `keybind` (`text:`, `esc:` and
//! `csi:` actions only, kept unparsed here for task 08), `theme` (resolved
//! by [`resolve_theme`], task 05) and `config-file`. Every other key is
//! ignored without a warning. A supported key whose value cannot be parsed
//! falls back to its default, keeps the other values, and produces a
//! [`Warning`] naming the file, line and reason (REQ-32).
//!
//! # Theme resolution (REQ-30)
//!
//! When `theme` names a theme, [`resolve_theme`] searches, in order: the
//! Subspace layer's `themes` directory, the Ghostty config `themes`
//! directory, then the Ghostty resources `themes` directory. An absolute
//! `theme` path loads only that file. `theme = light:A,dark:B` selects `A`
//! or `B` by the given [`Appearance`]. Theme colours apply first;
//! configuration colour keys override them regardless of line order. A
//! theme file's own `theme` and `config-file` lines are ignored. A missing
//! theme produces one warning and leaves the colour keys at their defaults.
//!
//! This crate's own `docs/terminal.md` table is maintained by the board
//! crate that depends on it, not here, because this crate's bounded files
//! are limited to `framework/runtimes/ghostty-config/`.

mod config;
mod keys;
mod parse;
mod paths;
mod theme;
mod theme_file;
mod types;
mod warning;

pub use config::GhosttyConfig;
pub use paths::Paths;
pub use theme::resolve_theme;
pub use types::{
    AdjustCellHeight, Appearance, ClipboardAccess, ConfirmCloseSurface, CopyOnSelect, CursorStyle,
    Rgb, ThemeSearchPaths, ThemeValues, WorkingDirectory,
};
pub use warning::Warning;

/// Reads the Ghostty configuration from `paths` and returns the resolved
/// values plus a warning for every supported key whose value could not be
/// parsed (REQ-29, REQ-32).
// qual:api
pub fn load(paths: &Paths) -> (GhosttyConfig, Vec<Warning>) {
    let roots = paths.ordered_config_files();
    let (entries, mut warnings) = parse::load_all(&roots);
    let (config, apply_warnings) = keys::apply_entries(&entries);
    warnings.extend(apply_warnings);
    (config, warnings)
}

#[cfg(test)]
mod tests {
    use super::{Paths, load};
    use std::path::PathBuf;

    #[test]
    fn load_returns_defaults_when_no_files_exist() {
        let paths = Paths {
            xdg_config_home: PathBuf::from("/no-such-home/.config"),
            macos_app_support: None,
            subspace_layer: PathBuf::from("/no-such-home/.config/subspace/ghostty"),
        };
        let (config, warnings) = load(&paths);
        assert!(warnings.is_empty());
        assert_eq!(config, Default::default());
    }

    #[test]
    fn load_applies_only_the_subspace_layer_when_it_is_the_only_file_present() {
        let dir = tempfile::tempdir().unwrap();
        let layer = dir.path().join("subspace-layer");
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(layer.join("config"), "font-size = 11\n").unwrap();

        let paths = Paths {
            xdg_config_home: dir.path().join("no-xdg-config"),
            macos_app_support: None,
            subspace_layer: layer,
        };
        let (config, warnings) = load(&paths);
        assert!(warnings.is_empty());
        assert_eq!(config.font_size, Some(11.0));
    }

    #[test]
    fn load_lets_the_subspace_layer_override_only_the_keys_it_sets() {
        let dir = tempfile::tempdir().unwrap();
        let xdg = dir.path().join("xdg-config/ghostty");
        let layer = dir.path().join("subspace-layer");
        std::fs::create_dir_all(&xdg).unwrap();
        std::fs::create_dir_all(&layer).unwrap();
        std::fs::write(xdg.join("config"), "font-size = 13\nterm = xterm-ghostty\n").unwrap();
        std::fs::write(layer.join("config"), "font-size = 11\n").unwrap();

        let paths = Paths {
            xdg_config_home: dir.path().join("xdg-config"),
            macos_app_support: None,
            subspace_layer: layer,
        };
        let (config, warnings) = load(&paths);
        assert!(warnings.is_empty());
        assert_eq!(config.font_size, Some(11.0));
        assert_eq!(config.term.as_deref(), Some("xterm-ghostty"));
    }
}
