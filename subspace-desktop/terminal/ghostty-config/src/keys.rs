//! Applies parsed [`crate::parse::Entry`] values to the REQ-31 supported
//! keys, producing a typed [`GhosttyConfig`] and a [`Warning`] for every
//! value a supported key cannot parse (REQ-32).

use crate::config::{
    DEFAULT_MOUSE_SCROLL_MULTIPLIER, DEFAULT_SCROLLBACK_LIMIT_BYTES, DEFAULT_WINDOW_PADDING,
    GhosttyConfig,
};
use crate::parse::Entry;
use crate::types::{
    AdjustCellHeight, ClipboardAccess, ConfirmCloseSurface, CopyOnSelect, CursorStyle, Rgb,
    WorkingDirectory,
};
use crate::warning::Warning;
use std::collections::BTreeMap;

/// Integration: applies every entry in order, producing the final
/// configuration and one warning per unparseable supported key (REQ-32).
pub(crate) fn apply_entries(entries: &[Entry]) -> (GhosttyConfig, Vec<Warning>) {
    let mut config = GhosttyConfig::default();
    let mut warnings = Vec::new();
    for entry in entries {
        apply_entry(&mut config, entry, &mut warnings);
    }
    (config, warnings)
}

/// Integration: routes one entry to its key's apply function, recording a
/// warning when parsing fails.
fn apply_entry(config: &mut GhosttyConfig, entry: &Entry, warnings: &mut Vec<Warning>) {
    let ok = apply_color_or_cursor_key(config, entry)
        .or_else(|| apply_font_or_scrollback_key(config, entry))
        .or_else(|| apply_clipboard_or_window_key(config, entry))
        .unwrap_or(true);
    if !ok {
        warnings.push(Warning::new(
            &entry.file,
            entry.line,
            format!("{}: cannot parse value", entry.key),
        ));
    }
}

/// Operation: applies a colour, palette or cursor key. Returns `None` for
/// any other key.
fn apply_color_or_cursor_key(config: &mut GhosttyConfig, entry: &Entry) -> Option<bool> {
    Some(match entry.key.as_str() {
        "background" => apply_color(&mut config.background, entry),
        "foreground" => apply_color(&mut config.foreground, entry),
        "cursor-color" => apply_color(&mut config.cursor_color, entry),
        "cursor-text" => apply_color(&mut config.cursor_text, entry),
        "selection-background" => apply_color(&mut config.selection_background, entry),
        "selection-foreground" => apply_color(&mut config.selection_foreground, entry),
        "palette" => apply_palette(&mut config.palette, entry),
        "cursor-style" => apply_parsed(&mut config.cursor_style, entry, CursorStyle::parse),
        "cursor-style-blink" => apply_bool(&mut config.cursor_style_blink, entry, true),
        _ => return None,
    })
}

/// Operation: applies a font, scrollback, shell or working-directory key.
/// Returns `None` for any other key.
fn apply_font_or_scrollback_key(config: &mut GhosttyConfig, entry: &Entry) -> Option<bool> {
    Some(match entry.key.as_str() {
        "font-family" => apply_font_family(&mut config.font_family, entry),
        "font-family-bold" => apply_font_family(&mut config.font_family_bold, entry),
        "font-family-italic" => apply_font_family(&mut config.font_family_italic, entry),
        "font-family-bold-italic" => apply_font_family(&mut config.font_family_bold_italic, entry),
        "font-size" => apply_option_parsed(&mut config.font_size, entry, |v| v.parse().ok()),
        "adjust-cell-height" => apply_option_parsed(
            &mut config.adjust_cell_height,
            entry,
            AdjustCellHeight::parse,
        ),
        "scrollback-limit" => apply_numeric(
            &mut config.scrollback_limit,
            entry,
            DEFAULT_SCROLLBACK_LIMIT_BYTES,
        ),
        "term" => apply_option_string(&mut config.term, entry),
        "command" => apply_option_string(&mut config.command, entry),
        "working-directory" => apply_option_parsed(
            &mut config.working_directory,
            entry,
            WorkingDirectory::parse,
        ),
        _ => return None,
    })
}

/// Operation: applies a clipboard, window or keybind key. Returns `None`
/// for any other key.
fn apply_clipboard_or_window_key(config: &mut GhosttyConfig, entry: &Entry) -> Option<bool> {
    Some(match entry.key.as_str() {
        "clipboard-read" => apply_parsed(&mut config.clipboard_read, entry, ClipboardAccess::parse),
        "clipboard-write" => {
            apply_parsed(&mut config.clipboard_write, entry, ClipboardAccess::parse)
        }
        "clipboard-paste-protection" => {
            apply_bool(&mut config.clipboard_paste_protection, entry, true)
        }
        "clipboard-paste-bracketed-safe" => {
            apply_bool(&mut config.clipboard_paste_bracketed_safe, entry, true)
        }
        "copy-on-select" => apply_copy_on_select(&mut config.copy_on_select, entry),
        "confirm-close-surface" => apply_option_parsed(
            &mut config.confirm_close_surface,
            entry,
            ConfirmCloseSurface::parse,
        ),
        "mouse-scroll-multiplier" => apply_numeric(
            &mut config.mouse_scroll_multiplier,
            entry,
            DEFAULT_MOUSE_SCROLL_MULTIPLIER,
        ),
        "window-padding-x" => {
            apply_numeric(&mut config.window_padding_x, entry, DEFAULT_WINDOW_PADDING)
        }
        "window-padding-y" => {
            apply_numeric(&mut config.window_padding_y, entry, DEFAULT_WINDOW_PADDING)
        }
        "keybind" => apply_keybind(&mut config.keybind, entry),
        "theme" => apply_option_string(&mut config.theme, entry),
        _ => return None,
    })
}

/// Operation: applies a colour key, resetting to unset on an empty value.
fn apply_color(field: &mut Option<Rgb>, entry: &Entry) -> bool {
    apply_option_parsed(field, entry, Rgb::parse)
}

/// Operation: applies a `palette = index=#rrggbb` line, resetting the whole
/// map on an empty value.
fn apply_palette(palette: &mut BTreeMap<u8, Rgb>, entry: &Entry) -> bool {
    let Some(value) = &entry.value else {
        palette.clear();
        return true;
    };
    let Some((index_str, color_str)) = value.split_once('=') else {
        return false;
    };
    let Ok(index) = index_str.trim().parse::<u8>() else {
        return false;
    };
    let Some(color) = Rgb::parse(color_str.trim()) else {
        return false;
    };
    palette.insert(index, color);
    true
}

/// Operation: appends one font family name, resetting the list on an empty
/// value.
fn apply_font_family(families: &mut Vec<String>, entry: &Entry) -> bool {
    match &entry.value {
        Some(value) => {
            families.push(value.clone());
            true
        }
        None => {
            families.clear();
            true
        }
    }
}

/// Operation: appends one raw `keybind` line, resetting the list on an
/// empty value. Task 08 interprets the action.
fn apply_keybind(keybinds: &mut Vec<String>, entry: &Entry) -> bool {
    match &entry.value {
        Some(value) => {
            keybinds.push(value.clone());
            true
        }
        None => {
            keybinds.clear();
            true
        }
    }
}

/// Operation: parses a required-default scalar, keeping the given default
/// on an empty value and reporting failure on an unparseable one.
fn apply_parsed<T: Copy>(field: &mut T, entry: &Entry, parse: impl Fn(&str) -> Option<T>) -> bool {
    match &entry.value {
        Some(value) => match parse(value) {
            Some(parsed) => {
                *field = parsed;
                true
            }
            None => false,
        },
        None => true,
    }
}

/// Operation: parses an optional scalar, clearing it to `None` on an empty
/// value and reporting failure on an unparseable one.
fn apply_option_parsed<T>(
    field: &mut Option<T>,
    entry: &Entry,
    parse: impl Fn(&str) -> Option<T>,
) -> bool {
    match &entry.value {
        Some(value) => match parse(value) {
            Some(parsed) => {
                *field = Some(parsed);
                true
            }
            None => false,
        },
        None => {
            *field = None;
            true
        }
    }
}

/// Operation: stores a raw string, clearing it to `None` on an empty value.
fn apply_option_string(field: &mut Option<String>, entry: &Entry) -> bool {
    *field = entry.value.clone();
    true
}

/// Operation: parses a `true`/`false` scalar, restoring the given default
/// on an empty value.
fn apply_bool(field: &mut bool, entry: &Entry, default: bool) -> bool {
    match &entry.value {
        Some(value) => match value.as_str() {
            "true" => {
                *field = true;
                true
            }
            "false" => {
                *field = false;
                true
            }
            _ => false,
        },
        None => {
            *field = default;
            true
        }
    }
}

/// Operation: parses a `FromStr` scalar (`f32` or `u64`), restoring the
/// given default on an empty value.
fn apply_numeric<T: std::str::FromStr + Copy>(field: &mut T, entry: &Entry, default: T) -> bool {
    match &entry.value {
        Some(value) => match value.parse::<T>() {
            Ok(parsed) => {
                *field = parsed;
                true
            }
            Err(_) => false,
        },
        None => {
            *field = default;
            true
        }
    }
}

/// Operation: parses `copy-on-select`, restoring the platform default on an
/// empty value.
fn apply_copy_on_select(field: &mut CopyOnSelect, entry: &Entry) -> bool {
    match &entry.value {
        Some(value) => match CopyOnSelect::parse(value) {
            Some(parsed) => {
                *field = parsed;
                true
            }
            None => false,
        },
        None => {
            *field = CopyOnSelect::platform_default();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::apply_entries;
    use crate::parse::Entry;
    use crate::types::Rgb;
    use std::path::PathBuf;

    fn entry(key: &str, value: Option<&str>, line: usize) -> Entry {
        Entry {
            key: key.to_string(),
            value: value.map(str::to_string),
            file: PathBuf::from("/config"),
            line,
        }
    }

    #[test]
    fn apply_entries_sets_a_supported_color_key() {
        let (config, warnings) = apply_entries(&[entry("background", Some("#112233"), 1)]);
        assert!(warnings.is_empty());
        assert_eq!(
            config.background,
            Some(Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            })
        );
    }

    #[test]
    fn apply_entries_ignores_an_unsupported_key_without_a_warning() {
        let (config, warnings) = apply_entries(&[entry("window-decoration", Some("false"), 1)]);
        assert!(warnings.is_empty());
        assert_eq!(config, Default::default());
    }

    #[test]
    fn apply_entries_warns_and_falls_back_to_default_on_an_invalid_value() {
        let (config, warnings) = apply_entries(&[entry("font-size", Some("big"), 3)]);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(config.font_size, None);
    }

    #[test]
    fn apply_entries_accumulates_repeated_font_family_lines() {
        let (config, warnings) = apply_entries(&[
            entry("font-family", Some("JetBrains Mono"), 1),
            entry("font-family", Some("Menlo"), 2),
        ]);
        assert!(warnings.is_empty());
        assert_eq!(
            config.font_family,
            vec!["JetBrains Mono".to_string(), "Menlo".to_string()]
        );
    }

    #[test]
    fn apply_entries_overrides_a_palette_index() {
        let (config, warnings) = apply_entries(&[entry("palette", Some("8=#888888"), 1)]);
        assert!(warnings.is_empty());
        assert_eq!(
            config.palette.get(&8),
            Some(&Rgb {
                r: 0x88,
                g: 0x88,
                b: 0x88
            })
        );
    }

    #[test]
    fn apply_entries_resets_to_default_on_an_empty_value() {
        let (config, _warnings) = apply_entries(&[
            entry("scrollback-limit", Some("1"), 1),
            entry("scrollback-limit", None, 2),
        ]);
        assert_eq!(config.scrollback_limit, 10_000_000);
    }
}
