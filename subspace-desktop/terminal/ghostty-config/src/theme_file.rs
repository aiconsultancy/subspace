//! Parses a theme file's colour keys, ignoring its own `theme` and
//! `config-file` lines (REQ-30).

use crate::parse::{parse_key_value, read_lines};
use crate::types::{Rgb, ThemeValues};
use std::path::Path;

/// Integration: reads a theme file and applies its supported colour keys,
/// skipping any value that fails to parse and keeping the rest.
pub(crate) fn parse_theme_file(path: &Path) -> ThemeValues {
    let mut values = ThemeValues::default();
    let Some(lines) = read_lines(path) else {
        return values;
    };
    for (_line, raw) in &lines {
        let Some((key, value)) = parse_key_value(raw) else {
            continue;
        };
        apply_theme_key(&mut values, &key, value.as_deref());
    }
    values
}

/// Operation: applies one theme colour key by name, ignoring `theme` and
/// `config-file` and any value that fails to parse.
fn apply_theme_key(values: &mut ThemeValues, key: &str, value: Option<&str>) {
    match key {
        "background" => values.background = value.and_then(Rgb::parse),
        "foreground" => values.foreground = value.and_then(Rgb::parse),
        "cursor-color" => values.cursor_color = value.and_then(Rgb::parse),
        "cursor-text" => values.cursor_text = value.and_then(Rgb::parse),
        "selection-background" => values.selection_background = value.and_then(Rgb::parse),
        "selection-foreground" => values.selection_foreground = value.and_then(Rgb::parse),
        "palette" => apply_theme_palette(values, value),
        _ => {}
    }
}

/// Operation: inserts one `index=#rrggbb` palette entry, ignoring an
/// unparseable one.
fn apply_theme_palette(values: &mut ThemeValues, value: Option<&str>) {
    let Some(value) = value else { return };
    let Some((index_str, color_str)) = value.split_once('=') else {
        return;
    };
    let Ok(index) = index_str.trim().parse::<u8>() else {
        return;
    };
    let Some(color) = Rgb::parse(color_str.trim()) else {
        return;
    };
    values.palette.insert(index, color);
}

#[cfg(test)]
mod tests {
    use super::parse_theme_file;
    use crate::types::Rgb;
    use std::path::Path;

    #[test]
    fn parse_theme_file_applies_background_and_palette() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme");
        std::fs::write(&path, "background = #112233\npalette = 8=#888888\n").unwrap();

        let values = parse_theme_file(&path);
        assert_eq!(
            values.background,
            Some(Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            })
        );
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
    fn parse_theme_file_returns_defaults_for_a_missing_file() {
        let values = parse_theme_file(Path::new("/no/such/theme"));
        assert_eq!(values.background, None);
    }
}
