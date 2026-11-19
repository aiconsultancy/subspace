//! The designer's colour fields: every theme colour the shared tokens read,
//! grouped by role, and their current values for keys a theme leaves derived.

use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::{Theme, try_parse_color};
use gpui_kit::{App, Entity, Hsla, Rgba, Window};
use subspace_dashboard::Tokens;

/// Theme colour keys by group, with their labels. Each one is read by
/// `Tokens`, GPUI Kit or both; the glass and flat button keys are Subspace
/// tones.
pub(super) const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Surfaces",
        &[
            ("background", "Page background"),
            ("muted.background", "Panel background"),
            ("popover.background", "Dialogs and menus"),
            ("title_bar.background", "Title bar"),
            ("sidebar.background", "Sidebar"),
            ("list.background", "List background"),
            ("secondary.background", "Secondary background"),
            ("group_box.background", "Group box"),
        ],
    ),
    (
        "Tabs",
        &[
            ("tab_bar.background", "Tab strip"),
            ("tab.background", "Tab"),
            ("tab.active.background", "Current tab"),
            ("tab.foreground", "Tab text"),
            ("tab.active.foreground", "Current tab text"),
        ],
    ),
    (
        "Status bar",
        &[
            ("status_bar.background", "Status bar"),
            ("status_bar.border", "Status bar edge"),
        ],
    ),
    (
        "Inputs",
        // GPUI Kit fills inputs in dark themes with this colour at 30%.
        &[("input.border", "Input border and dark fill")],
    ),
    (
        "Lines",
        &[
            ("border", "Separator"),
            ("title_bar.border", "Title bar edge"),
            ("sidebar.border", "Sidebar edge"),
            ("list.hover.background", "Row hover"),
        ],
    ),
    (
        "Text",
        &[
            ("foreground", "Body text"),
            ("caret", "Strong text"),
            ("muted.foreground", "Muted text"),
        ],
    ),
    (
        "Actions and selection",
        &[
            ("primary.background", "Primary action"),
            ("primary.foreground", "Primary text"),
            ("accent.background", "Selected background"),
            ("accent.foreground", "Selected text"),
            ("ring", "Focus indicator"),
        ],
    ),
    (
        "Status",
        &[
            ("danger.background", "Danger"),
            ("warning.background", "Warning"),
            ("success.background", "Success"),
            ("info.background", "Information"),
        ],
    ),
    (
        "Glass",
        &[
            ("glass", "Glass fill"),
            ("glass-rim", "Glass rim"),
            ("glass-shadow", "Glass shadow"),
        ],
    ),
    (
        "Flat buttons",
        &[("button-hover", "Button hover"), ("button-on", "Button on")],
    ),
];

/// One colour, edited with GPUI Kit's colour select, and the hex value it
/// showed after the last sync, so the designer writes only colours the theme
/// sets or the person changed.
pub(super) struct ColorField {
    pub key: &'static str,
    pub label: &'static str,
    pub picker: Entity<ColorPickerState>,
    pub synced: String,
}

impl ColorField {
    /// Show `value` without a change event; `synced` keeps it as written.
    pub(super) fn show(&mut self, value: String, window: &mut Window, cx: &mut App) {
        let color = try_parse_color(&value).ok();
        self.synced = color.map_or(value, hex);
        self.picker.update(cx, |picker, cx| match color {
            Some(color) => picker.set_value(color, window, cx),
            None => picker.clear_value(window, cx),
        });
    }

    /// The colour as hex, or empty when none is set.
    pub(super) fn value(&self, cx: &App) -> String {
        self.picker.read(cx).value().map(hex).unwrap_or_default()
    }
}

/// Every key in [`GROUPS`], in order.
pub(super) fn keys() -> impl Iterator<Item = (&'static str, &'static str)> {
    GROUPS.iter().flat_map(|(_, fields)| fields.iter().copied())
}

/// The colour in effect for `key`, as hex.
pub(super) fn current(key: &str, cx: &App) -> String {
    current_color(key, Theme::global(cx), &Tokens::current(cx))
        .map(hex)
        .unwrap_or_default()
}

/// The largest value of an 8-bit colour channel.
const CHANNEL_MAX: f32 = 255.;
/// Full alpha: an opaque colour writes no alpha byte.
const OPAQUE: f32 = 1.;

/// `color` as `#rrggbb`, or `#rrggbbaa` when translucent. Each channel is
/// rounded, so a colour read from hex writes the same hex back; GPUI Kit's
/// `to_hex` truncates and can drop a step.
pub(super) fn hex(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    // `as u8` saturates, so a channel just outside 0..=1 still writes 00 or ff.
    let channel = |value: f32| (value * CHANNEL_MAX).round() as u8;
    let rgb = format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    );
    if rgba.a < OPAQUE {
        format!("{rgb}{:02x}", channel(rgba.a))
    } else {
        rgb
    }
}

fn current_color(key: &str, theme: &Theme, tokens: &Tokens) -> Option<Hsla> {
    let color = match key {
        "background" => theme.background,
        "foreground" => theme.foreground,
        "muted.background" => theme.muted,
        "muted.foreground" => theme.muted_foreground,
        "popover.background" => theme.popover,
        "title_bar.background" => theme.title_bar,
        "title_bar.border" => theme.title_bar_border,
        "sidebar.background" => theme.sidebar,
        "sidebar.border" => theme.sidebar_border,
        "list.hover.background" => theme.list_hover,
        "list.background" => theme.colors.list,
        "secondary.background" => theme.secondary,
        "group_box.background" => theme.group_box,
        "tab_bar.background" => theme.tab_bar,
        "tab.background" => theme.tab,
        "tab.active.background" => theme.tab_active,
        "tab.foreground" => theme.tab_foreground,
        "tab.active.foreground" => theme.tab_active_foreground,
        "status_bar.background" => theme.status_bar,
        "status_bar.border" => theme.status_bar_border,
        "input.border" => theme.input,
        "border" => theme.border,
        "caret" => theme.caret,
        "ring" => theme.ring,
        "primary.background" => theme.primary,
        "primary.foreground" => theme.primary_foreground,
        "accent.background" => theme.accent,
        "accent.foreground" => theme.accent_foreground,
        "danger.background" => theme.danger,
        "warning.background" => theme.warning,
        "success.background" => theme.success,
        "info.background" => theme.info,
        "glass" => tokens.glass,
        "glass-rim" => tokens.glass_rim,
        "glass-shadow" => tokens.glass_shadow,
        "button-hover" => tokens.button_hover,
        "button-on" => tokens.button_on,
        _ => return None,
    };
    Some(color)
}

#[cfg(test)]
mod tests {
    use super::{current_color, keys};
    use crate::theme::ThemeDocument;
    use gpui_kit::component::Theme;
    use subspace_dashboard::Tokens;

    #[test]
    fn hex_round_trips_every_written_colour() {
        use gpui_kit::component::try_parse_color;
        for value in [
            "#123456",
            "#0969da",
            "#ffffff",
            "#000000",
            "#4c8dff52",
            "#00000000",
        ] {
            assert_eq!(super::hex(try_parse_color(value).unwrap()), value);
        }
    }

    #[test]
    fn every_field_is_a_theme_colour_with_a_current_value() {
        let theme = Theme::default();
        let tokens = Tokens::one_dark();
        let mut document = ThemeDocument::parse(crate::theme::THEME_JSON).unwrap();
        for (key, label) in keys() {
            assert!(current_color(key, &theme, &tokens).is_some(), "{label}");
            document.set_color(key, "#123456").unwrap();
            assert_eq!(document.color(key).as_deref(), Some("#123456"), "{label}");
        }
    }
}
