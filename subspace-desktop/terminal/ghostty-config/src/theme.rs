//! Theme resolution: a named `theme` searches the Subspace layer, the
//! Ghostty config directory and the Ghostty resources directory, in that
//! order (REQ-30). Configuration colour keys apply on top of the theme
//! regardless of line order.

use crate::config::GhosttyConfig;
use crate::theme_file::parse_theme_file;
use crate::types::{Appearance, ThemeSearchPaths, ThemeValues};
use crate::warning::Warning;
use std::path::{Path, PathBuf};

/// Resolves `config.theme` for `appearance`, applies its colours beneath
/// `config`'s own colour keys, and returns the result plus a warning when
/// the theme cannot be found (REQ-30, REQ-32).
pub fn resolve_theme(
    config: &GhosttyConfig,
    appearance: Appearance,
    search_paths: &ThemeSearchPaths,
) -> (ThemeValues, Vec<Warning>) {
    let mut warnings = Vec::new();
    let mut values = match &config.theme {
        Some(raw) => load_named_theme(raw, appearance, search_paths, &mut warnings),
        None => ThemeValues::default(),
    };
    overlay_config_colors(&mut values, config);
    (values, warnings)
}

/// Integration: picks the theme name for `appearance`, finds its file and
/// parses its colours, or returns defaults with a warning when it is
/// missing.
fn load_named_theme(
    raw: &str,
    appearance: Appearance,
    search_paths: &ThemeSearchPaths,
    warnings: &mut Vec<Warning>,
) -> ThemeValues {
    let name = selected_theme_name(raw, appearance);
    let Some(path) = find_theme_file(&name, search_paths) else {
        warnings.push(Warning::new(
            "theme",
            0,
            format!("theme {name:?}: not found"),
        ));
        return ThemeValues::default();
    };
    parse_theme_file(&path)
}

/// Operation: picks the `light:`/`dark:` half of a theme value for
/// `appearance`, or returns a plain name unchanged.
fn selected_theme_name(raw: &str, appearance: Appearance) -> String {
    let has_pair_syntax = raw.contains(',') || raw.contains('=') || raw.contains(':');
    if !has_pair_syntax {
        return raw.trim().to_string();
    }
    let mut light = None;
    let mut dark = None;
    for part in raw.split(',') {
        if let Some((key, value)) = part.split_once(':') {
            match key.trim() {
                "light" => light = Some(value.trim().to_string()),
                "dark" => dark = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }
    match appearance {
        Appearance::Light => light.or(dark).unwrap_or_else(|| raw.trim().to_string()),
        Appearance::Dark => dark.or(light).unwrap_or_else(|| raw.trim().to_string()),
    }
}

/// Operation: an absolute name resolves to that file only; otherwise
/// searches the Subspace layer, then the Ghostty config directory, then the
/// Ghostty resources directory.
fn find_theme_file(name: &str, search_paths: &ThemeSearchPaths) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    [
        Some(&search_paths.subspace_layer_themes),
        Some(&search_paths.xdg_themes),
        search_paths.resources_themes.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|dir| dir.join(name))
    .find(|path| path.is_file())
}

/// Operation: overrides the theme's colours with the configuration's own
/// colour keys, which win regardless of line order (REQ-30).
fn overlay_config_colors(values: &mut ThemeValues, config: &GhosttyConfig) {
    if config.background.is_some() {
        values.background = config.background;
    }
    if config.foreground.is_some() {
        values.foreground = config.foreground;
    }
    if config.cursor_color.is_some() {
        values.cursor_color = config.cursor_color;
    }
    if config.cursor_text.is_some() {
        values.cursor_text = config.cursor_text;
    }
    if config.selection_background.is_some() {
        values.selection_background = config.selection_background;
    }
    if config.selection_foreground.is_some() {
        values.selection_foreground = config.selection_foreground;
    }
    for (index, color) in &config.palette {
        values.palette.insert(*index, *color);
    }
}

#[cfg(test)]
mod tests {
    use super::{Appearance, ThemeSearchPaths, ThemeValues, resolve_theme};
    use crate::config::GhosttyConfig;
    use crate::types::Rgb;

    fn search_paths(dir: &std::path::Path) -> ThemeSearchPaths {
        ThemeSearchPaths {
            subspace_layer_themes: dir.join("subspace-layer-themes"),
            xdg_themes: dir.join("xdg-themes"),
            resources_themes: None,
        }
    }

    /// Writes a theme file under a fresh temp directory's xdg themes
    /// directory and returns the directory.
    fn write_xdg_theme(name: &str, content: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let xdg_themes = dir.path().join("xdg-themes");
        std::fs::create_dir_all(&xdg_themes).unwrap();
        std::fs::write(xdg_themes.join(name), content).unwrap();
        dir
    }

    /// Resolves `config` against the fixture search paths with
    /// `Appearance::Dark`, asserting there are no warnings, and returns the
    /// resolved values.
    fn resolve_ok(dir: &std::path::Path, config: &GhosttyConfig) -> ThemeValues {
        let (values, warnings) = resolve_theme(config, Appearance::Dark, &search_paths(dir));
        assert!(warnings.is_empty());
        values
    }

    /// Asserts that `values.background` is the `#112233` colour both
    /// fixture themes below set.
    fn assert_background_is_112233(values: &ThemeValues) {
        assert_eq!(
            values.background,
            Some(Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            })
        );
    }

    /// Writes a theme with the given name and content, resolves it with a
    /// config naming that theme, and asserts the result is `#112233`
    /// background with no warnings. Shared by two acceptance tests that
    /// check different theme-file content produces the same colour.
    fn assert_theme_resolves_to_112233_background(name: &str, content: &str) {
        let dir = write_xdg_theme(name, content);
        let config = GhosttyConfig {
            theme: Some(name.to_string()),
            ..Default::default()
        };
        let values = resolve_ok(dir.path(), &config);
        assert_background_is_112233(&values);
    }

    #[test]
    fn resolve_theme_loads_a_named_theme_from_the_xdg_themes_directory() {
        assert_theme_resolves_to_112233_background(
            "One Double Dark Remix",
            "background = #112233\n",
        );
    }

    #[test]
    fn resolve_theme_prefers_an_explicit_palette_override_over_the_theme_line_order() {
        let dir = write_xdg_theme("Remix", "palette = 8=#111111\n");

        let mut config = GhosttyConfig {
            theme: Some("Remix".to_string()),
            ..Default::default()
        };
        config.palette.insert(
            8,
            Rgb {
                r: 0x88,
                g: 0x88,
                b: 0x88,
            },
        );
        let (values, _warnings) =
            resolve_theme(&config, Appearance::Dark, &search_paths(dir.path()));
        assert_eq!(
            values.palette.get(&8),
            Some(&Rgb {
                r: 0x88,
                g: 0x88,
                b: 0x88
            })
        );
    }

    #[test]
    fn resolve_theme_picks_the_pair_half_matching_the_appearance() {
        let dir = tempfile::tempdir().unwrap();
        let xdg_themes = dir.path().join("xdg-themes");
        std::fs::create_dir_all(&xdg_themes).unwrap();
        std::fs::write(xdg_themes.join("Day"), "background = #ffffff\n").unwrap();
        std::fs::write(xdg_themes.join("Night"), "background = #000000\n").unwrap();

        let config = GhosttyConfig {
            theme: Some("light:Day,dark:Night".to_string()),
            ..Default::default()
        };
        let (values, warnings) =
            resolve_theme(&config, Appearance::Light, &search_paths(dir.path()));
        assert!(warnings.is_empty());
        assert_eq!(
            values.background,
            Some(Rgb {
                r: 0xff,
                g: 0xff,
                b: 0xff
            })
        );
    }

    #[test]
    fn resolve_theme_warns_once_on_a_missing_theme_and_keeps_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = GhosttyConfig {
            theme: Some("Missing".to_string()),
            term: Some("xterm-ghostty".to_string()),
            ..Default::default()
        };
        let (values, warnings) =
            resolve_theme(&config, Appearance::Dark, &search_paths(dir.path()));
        assert_eq!(warnings.len(), 1);
        assert_eq!(values.background, None);
        assert_eq!(config.term.as_deref(), Some("xterm-ghostty"));
    }

    #[test]
    fn resolve_theme_ignores_a_theme_files_own_theme_and_config_file_lines() {
        assert_theme_resolves_to_112233_background(
            "Remix",
            "theme = Other\nconfig-file = other\nbackground = #112233\n",
        );
    }

    #[test]
    fn resolve_theme_loads_an_absolute_path_theme_directly() {
        let dir = tempfile::tempdir().unwrap();
        let theme_path = dir.path().join("my-theme");
        std::fs::write(&theme_path, "foreground = #abcdef\n").unwrap();

        let config = GhosttyConfig {
            theme: Some(theme_path.to_string_lossy().to_string()),
            ..Default::default()
        };
        let values = resolve_ok(dir.path(), &config);
        assert_eq!(
            values.foreground,
            Some(Rgb {
                r: 0xab,
                g: 0xcd,
                b: 0xef
            })
        );
    }
}
