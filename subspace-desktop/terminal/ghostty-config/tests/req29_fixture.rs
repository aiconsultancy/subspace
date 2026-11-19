//! REQ-29/REQ-31/REQ-32 fixture tests: file load order, overrides, includes
//! and invalid-value warnings, checked against a committed capture of
//! `ghostty +show-config` for the REQ-31 keys.

use std::path::{Path, PathBuf};
use subspace_ghostty_config::{
    ClipboardAccess, ConfirmCloseSurface, CopyOnSelect, CursorStyle, Paths, WorkingDirectory, load,
};

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn req29_paths() -> Paths {
    let root = fixtures_root().join("req29");
    Paths {
        xdg_config_home: root.join("xdg-config"),
        macos_app_support: Some(root.join("macos-app-support")),
        subspace_layer: root.join("subspace-layer"),
    }
}

/// Returns the first `key = value` line's value from a captured
/// `ghostty +show-config` file.
fn captured_value<'a>(capture: &'a str, key: &str) -> &'a str {
    capture
        .lines()
        .find_map(|line| {
            let (found_key, value) = line.split_once(" = ")?;
            (found_key == key).then_some(value)
        })
        .unwrap_or_else(|| panic!("capture has no {key} line"))
}

#[test]
fn fixture_matches_the_ghostty_capture_for_keys_the_layers_do_not_override() {
    let capture = std::fs::read_to_string(fixtures_root().join("req29/ghostty-show-config.txt"))
        .expect("committed ghostty +show-config capture");
    let (config, warnings) = load(&req29_paths());
    assert!(warnings.is_empty());

    assert_eq!(
        format!(
            "#{:02x}{:02x}{:02x}",
            config.background.unwrap().r,
            config.background.unwrap().g,
            config.background.unwrap().b
        ),
        captured_value(&capture, "background")
    );
    assert_eq!(
        format!(
            "#{:02x}{:02x}{:02x}",
            config.foreground.unwrap().r,
            config.foreground.unwrap().g,
            config.foreground.unwrap().b
        ),
        captured_value(&capture, "foreground")
    );
    let palette_8 = config.palette.get(&8).unwrap();
    assert_eq!(
        format!("#{:02x}{:02x}{:02x}", palette_8.r, palette_8.g, palette_8.b),
        "#928374"
    );
    assert_eq!(config.cursor_style, CursorStyle::Bar);
    assert_eq!(captured_value(&capture, "cursor-style"), "bar");
    assert!(!config.cursor_style_blink);
    assert_eq!(captured_value(&capture, "cursor-style-blink"), "false");
    assert_eq!(config.scrollback_limit, 5_000_000);
    assert_eq!(captured_value(&capture, "scrollback-limit"), "5000000");
    assert_eq!(config.term.as_deref(), Some("xterm-ghostty-subspace"));
    assert_eq!(captured_value(&capture, "term"), "xterm-ghostty-subspace");
    assert_eq!(config.command.as_deref(), Some("/bin/zsh"));
    assert_eq!(config.working_directory, Some(WorkingDirectory::Home));
    assert_eq!(config.clipboard_read, ClipboardAccess::Allow);
    assert_eq!(config.clipboard_write, ClipboardAccess::Deny);
    assert!(!config.clipboard_paste_protection);
    assert!(!config.clipboard_paste_bracketed_safe);
    assert_eq!(config.copy_on_select, CopyOnSelect::Clipboard);
    assert_eq!(
        config.confirm_close_surface,
        Some(ConfirmCloseSurface::Always)
    );
    assert_eq!(config.mouse_scroll_multiplier, 2.5);
    assert_eq!(config.window_padding_y, 6.0);
    let adjust = match config.adjust_cell_height {
        Some(subspace_ghostty_config::AdjustCellHeight::Percent(value)) => value,
        other => panic!("expected a percentage, got {other:?}"),
    };
    assert!((adjust - 10.0).abs() < 0.001);
}

#[test]
fn a_configuration_setting_ghostty_only_window_keys_loads_without_a_warning() {
    // window-decoration, custom-shader and macos-titlebar-style are in the
    // xdg fixture config and are not in REQ-31, so they must be ignored
    // silently.
    let (_config, warnings) = load(&req29_paths());
    assert!(warnings.is_empty());
}

#[test]
fn the_subspace_layer_overrides_only_font_size() {
    let (config, warnings) = load(&req29_paths());
    assert!(warnings.is_empty());
    assert_eq!(config.font_size, Some(15.0));
    // Everything else still traces back to the xdg file.
    assert_eq!(config.term.as_deref(), Some("xterm-ghostty-subspace"));
}

#[test]
fn with_only_the_subspace_layer_present_its_values_apply() {
    let root = fixtures_root().join("req29");
    let paths = Paths {
        xdg_config_home: root.join("no-such-xdg-config"),
        macos_app_support: None,
        subspace_layer: root.join("subspace-layer"),
    };
    let (config, warnings) = load(&paths);
    assert!(warnings.is_empty());
    assert_eq!(config.font_size, Some(15.0));
    assert_eq!(config.term, None);
}

#[test]
fn an_optional_include_applies_after_the_whole_including_file() {
    let (config, warnings) = load(&req29_paths());
    assert!(warnings.is_empty());
    // The xdg config sets window-padding-x = 4; its optional include then
    // sets it to 9, and that include must apply after the whole file.
    assert_eq!(config.window_padding_x, 9.0);
}

#[test]
fn an_invalid_value_produces_exactly_one_warning_naming_the_file_and_line_and_keeps_the_default() {
    let root = fixtures_root().join("invalid-value");
    let paths = Paths {
        xdg_config_home: root.clone(),
        macos_app_support: None,
        subspace_layer: fixtures_root().join("req29/no-such-subspace-layer"),
    };
    let (config, warnings) = load(&paths);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].file, root.join("ghostty/config"));
    assert_eq!(warnings[0].line, 1);
    assert_eq!(config.font_size, None);
}

#[test]
fn an_include_cycle_produces_one_warning_and_terminates() {
    let root = fixtures_root().join("include-cycle");
    let paths = Paths {
        xdg_config_home: root,
        macos_app_support: None,
        subspace_layer: fixtures_root().join("req29/no-such-subspace-layer"),
    };
    let (_config, warnings) = load(&paths);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].reason.contains("cycle"));
}
