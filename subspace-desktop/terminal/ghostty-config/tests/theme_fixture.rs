//! REQ-30 fixture tests: named theme search order, absolute-path themes,
//! light/dark pairs, configuration overrides and a missing theme.

use std::path::{Path, PathBuf};
use subspace_ghostty_config::{Appearance, GhosttyConfig, Rgb, ThemeSearchPaths, resolve_theme};

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/themes")
}

fn search_paths() -> ThemeSearchPaths {
    let root = fixtures_root();
    ThemeSearchPaths {
        subspace_layer_themes: root.join("subspace-layer-themes"),
        xdg_themes: root.join("xdg-themes"),
        resources_themes: None,
    }
}

#[test]
fn a_named_theme_loads_from_the_ghostty_config_themes_directory() {
    let config = GhosttyConfig {
        theme: Some("One Double Dark Remix".to_string()),
        ..Default::default()
    };
    let (values, warnings) = resolve_theme(&config, Appearance::Dark, &search_paths());
    assert!(warnings.is_empty());
    assert_eq!(
        values.background,
        Some(Rgb {
            r: 0x1d,
            g: 0x20,
            b: 0x21
        })
    );
}

#[test]
fn a_config_palette_override_wins_over_the_theme_even_when_theme_comes_later() {
    let config = GhosttyConfig {
        theme: Some("One Double Dark Remix".to_string()),
        palette: [(
            8,
            Rgb {
                r: 0x88,
                g: 0x88,
                b: 0x88,
            },
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    let (values, _warnings) = resolve_theme(&config, Appearance::Dark, &search_paths());
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
fn a_light_dark_pair_selects_by_the_supplied_appearance() {
    let config = GhosttyConfig {
        theme: Some("light:Shell Light,dark:Shell Dark".to_string()),
        ..Default::default()
    };

    let (light, warnings) = resolve_theme(&config, Appearance::Light, &search_paths());
    assert!(warnings.is_empty());
    assert_eq!(
        light.background,
        Some(Rgb {
            r: 0xff,
            g: 0xff,
            b: 0xff
        })
    );

    let (dark, warnings) = resolve_theme(&config, Appearance::Dark, &search_paths());
    assert!(warnings.is_empty());
    assert_eq!(
        dark.background,
        Some(Rgb {
            r: 0x00,
            g: 0x00,
            b: 0x00
        })
    );
}

#[test]
fn a_missing_theme_yields_one_warning_and_default_colours_and_the_rest_survives() {
    let config = GhosttyConfig {
        theme: Some("Missing".to_string()),
        term: Some("xterm-ghostty".to_string()),
        ..Default::default()
    };
    let (values, warnings) = resolve_theme(&config, Appearance::Dark, &search_paths());
    assert_eq!(warnings.len(), 1);
    assert_eq!(values.background, None);
    assert_eq!(config.term.as_deref(), Some("xterm-ghostty"));
}

#[test]
fn an_absolute_theme_path_loads_only_that_file() {
    let path = fixtures_root().join("absolute/custom-theme");
    let config = GhosttyConfig {
        theme: Some(path.to_string_lossy().to_string()),
        ..Default::default()
    };
    let (values, warnings) = resolve_theme(&config, Appearance::Dark, &search_paths());
    assert!(warnings.is_empty());
    assert_eq!(
        values.cursor_color,
        Some(Rgb {
            r: 0xff,
            g: 0x00,
            b: 0xff
        })
    );
}
