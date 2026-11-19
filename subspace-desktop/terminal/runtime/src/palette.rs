//! The colours a session draws with (REQ-12).
//!
//! Ghostty's configuration and theme win wherever they set a colour. Every
//! other colour derives from `subspace_framework::dashboard::tokens`, so changing the
//! Subspace theme recolours open sessions without a restart: the board builds a
//! new [`crate::TerminalStyle`] and calls [`crate::TerminalView::apply_config`].
//!
//! The derivation REQ-12 asks for, as amended on 2026-10-13: the default
//! foreground and background are the `fg` and `bg` tokens; ANSI black and
//! white are the background and foreground tones; the six hues are the
//! matching tokens; each bright colour is a lighter version of the same hue,
//! and bright white is `fg_strong`.
//!
//! The 256-colour palette goes into the emulator when a session starts, so
//! libghostty resolves every indexed colour itself
//! and the renderer only ever reads resolved values. Indices 16 to 255 keep
//! Ghostty's own cube and grey ramp unless the configuration or the theme
//! names one, and 24-bit direct colour never passes through the palette at
//! all, so it renders exactly as requested.

use std::collections::BTreeMap;

use gpui_kit::{Hsla, Rgba};
use libghostty_vt::Terminal;
use libghostty_vt::style::{Palette, PaletteIndex, RgbColor};
use subspace_framework::dashboard::tokens::Tokens;
use subspace_ghostty_config::{GhosttyConfig, Rgb, ThemeValues};

use crate::error::{SessionError, emulator_error};

/// The ANSI colours, the first 16 palette entries.
pub const ANSI_COLORS: usize = 16;

/// How much lighter a bright ANSI colour is than its normal half. A fixed
/// lightness lift keeps the hue, which is what REQ-12 asks for; the tokens
/// carry one shade per hue, so there is no strong variant to read.
const BRIGHT_LIFT: f32 = 0.15;

/// The largest value of one 8-bit colour channel.
const CHANNEL_MAX: f32 = 255.0;

/// The colours one session draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalPalette {
    /// The default background, behind every cell that sets none.
    pub background: Rgb,
    /// The default foreground, for every cell that sets none.
    pub foreground: Rgb,
    /// The cursor's own colour.
    pub cursor: Rgb,
    /// The colour of the glyph a filled cursor covers.
    pub cursor_text: Rgb,
    /// The background of a selected cell.
    pub selection_background: Rgb,
    /// The foreground of a selected cell.
    pub selection_foreground: Rgb,
    /// The 16 ANSI colours, in the usual order: black, red, green, yellow,
    /// blue, magenta, cyan, white, then the bright half.
    pub ansi: [Rgb; ANSI_COLORS],
}

/// The colour keys a theme file or the configuration can set (REQ-12).
///
/// Both carry the same seven colours and a palette, so both are read through
/// this one shape and applied with the same rule.
struct ColorKeys<'a> {
    background: Option<Rgb>,
    foreground: Option<Rgb>,
    cursor: Option<Rgb>,
    cursor_text: Option<Rgb>,
    selection_background: Option<Rgb>,
    selection_foreground: Option<Rgb>,
    palette: &'a BTreeMap<u8, Rgb>,
}

impl<'a> ColorKeys<'a> {
    /// The colours a theme file set (REQ-30).
    fn of_theme(theme: &'a ThemeValues) -> Self {
        Self {
            background: theme.background,
            foreground: theme.foreground,
            cursor: theme.cursor_color,
            cursor_text: theme.cursor_text,
            selection_background: theme.selection_background,
            selection_foreground: theme.selection_foreground,
            palette: &theme.palette,
        }
    }

    /// The colours the configuration set (REQ-31).
    fn of_config(config: &'a GhosttyConfig) -> Self {
        Self {
            background: config.background,
            foreground: config.foreground,
            cursor: config.cursor_color,
            cursor_text: config.cursor_text,
            selection_background: config.selection_background,
            selection_foreground: config.selection_foreground,
            palette: &config.palette,
        }
    }
}

impl TerminalPalette {
    /// The palette for a session: Ghostty's values over the derived tokens.
    ///
    /// Theme colours apply first and configuration colour keys override them,
    /// whatever their line order, as REQ-30 requires.
    pub fn resolve(config: &GhosttyConfig, theme: &ThemeValues, tokens: &Tokens) -> Self {
        let derived = Self::from_tokens(tokens);
        let themed = derived.with_keys(&ColorKeys::of_theme(theme));
        themed.with_keys(&ColorKeys::of_config(config))
    }

    /// The palette the active Subspace theme alone produces (REQ-12).
    fn from_tokens(tokens: &Tokens) -> Self {
        Self {
            background: to_rgb(tokens.bg),
            foreground: to_rgb(tokens.fg),
            cursor: to_rgb(tokens.fg),
            cursor_text: to_rgb(tokens.bg),
            selection_background: to_rgb(tokens.select),
            selection_foreground: to_rgb(tokens.fg_strong),
            ansi: ansi_from_tokens(tokens),
        }
    }

    /// The palette with one set of colour keys applied: every key that is
    /// set replaces the colour below it, and every key that is not leaves it.
    fn with_keys(self, keys: &ColorKeys<'_>) -> Self {
        Self {
            background: keys.background.unwrap_or(self.background),
            foreground: keys.foreground.unwrap_or(self.foreground),
            cursor: keys.cursor.unwrap_or(self.cursor),
            cursor_text: keys.cursor_text.unwrap_or(self.cursor_text),
            selection_background: keys
                .selection_background
                .unwrap_or(self.selection_background),
            selection_foreground: keys
                .selection_foreground
                .unwrap_or(self.selection_foreground),
            ansi: with_palette_entries(self.ansi, keys.palette),
        }
    }
}

/// Give the emulator the palette, so it resolves indexed colours itself.
///
/// The indices the configuration and the theme name above 15 go in as well;
/// every other index keeps Ghostty's own colour cube and grey ramp.
pub(crate) fn apply_to_terminal(
    palette: &TerminalPalette,
    indexed: &BTreeMap<u8, Rgb>,
    terminal: &mut Terminal<'static, 'static>,
) -> Result<(), SessionError> {
    let full = full_palette(palette, indexed);
    terminal
        .set_default_color_palette(Some(full))
        .map_err(emulator_error)?;
    terminal
        .set_default_fg_color(Some(to_color(palette.foreground)))
        .map_err(emulator_error)?;
    terminal
        .set_default_bg_color(Some(to_color(palette.background)))
        .map_err(emulator_error)?;
    terminal
        .set_default_cursor_color(Some(to_color(palette.cursor)))
        .map_err(emulator_error)?;
    Ok(())
}

/// The 256 entries the emulator resolves indexed colours through.
fn full_palette(palette: &TerminalPalette, indexed: &BTreeMap<u8, Rgb>) -> Palette {
    let mut full = Palette::default();
    for (index, color) in palette.ansi.iter().enumerate() {
        full.0[index] = to_color(*color);
    }
    for (index, color) in indexed {
        full.set(PaletteIndex(*index), to_color(*color));
    }
    full
}

/// The 16 ANSI colours the tokens produce (REQ-12).
fn ansi_from_tokens(tokens: &Tokens) -> [Rgb; ANSI_COLORS] {
    let normal = [
        tokens.bg,
        tokens.red,
        tokens.green,
        tokens.yellow,
        tokens.blue,
        tokens.magenta,
        tokens.cyan,
        tokens.fg,
    ];
    let mut ansi = [to_rgb(tokens.bg); ANSI_COLORS];
    for (index, color) in normal.into_iter().enumerate() {
        ansi[index] = to_rgb(color);
        ansi[index + normal.len()] = to_rgb(lighter(color));
    }
    ansi[ANSI_COLORS - 1] = to_rgb(tokens.fg_strong);
    ansi
}

/// The ANSI colours with every entry a palette key names below 16.
fn with_palette_entries(
    mut ansi: [Rgb; ANSI_COLORS],
    entries: &BTreeMap<u8, Rgb>,
) -> [Rgb; ANSI_COLORS] {
    for (index, color) in entries {
        if let Some(entry) = ansi.get_mut(usize::from(*index)) {
            *entry = *color;
        }
    }
    ansi
}

/// The bright half of a hue: the same colour, lifted in lightness.
fn lighter(color: Hsla) -> Hsla {
    Hsla {
        l: (color.l + BRIGHT_LIFT).min(1.0),
        ..color
    }
}

/// A theme colour as the 8-bit channels the emulator and the files use.
fn to_rgb(color: Hsla) -> Rgb {
    let rgba = Rgba::from(color);
    Rgb {
        r: channel(rgba.r),
        g: channel(rgba.g),
        b: channel(rgba.b),
    }
}

/// One channel, rounded from GPUI's `0.0` to `1.0` range.
fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * CHANNEL_MAX).round() as u8
}

/// A colour as libghostty takes it.
pub(crate) fn to_color(color: Rgb) -> RgbColor {
    RgbColor {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

/// A colour libghostty reported, as GPUI paints it.
pub(crate) fn to_hsla(color: RgbColor) -> Hsla {
    Hsla::from(Rgba {
        r: f32::from(color.r) / CHANNEL_MAX,
        g: f32::from(color.g) / CHANNEL_MAX,
        b: f32::from(color.b) / CHANNEL_MAX,
        a: 1.0,
    })
}

/// One of the palette's own colours, as GPUI paints it.
pub(crate) fn paint_color(color: Rgb) -> Hsla {
    to_hsla(to_color(color))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use subspace_framework::dashboard::tokens::Tokens;
    use subspace_ghostty_config::{GhosttyConfig, Rgb, ThemeValues};

    use crate::palette::{TerminalPalette, paint_color, to_hsla};

    /// A colour that no token produces, so a test can tell it apart.
    const MARKER: Rgb = Rgb {
        r: 0x12,
        g: 0x34,
        b: 0x56,
    };

    /// The palette the tokens alone produce.
    fn derived() -> TerminalPalette {
        TerminalPalette::resolve(
            &GhosttyConfig::default(),
            &ThemeValues::default(),
            &Tokens::one_dark(),
        )
    }

    #[test]
    fn the_defaults_and_the_ansi_ends_come_from_the_tokens() {
        let tokens = Tokens::one_dark();
        let palette = derived();

        assert_eq!(paint_color(palette.background), tokens.bg);
        assert_eq!(paint_color(palette.foreground), tokens.fg);
        assert_eq!(paint_color(palette.ansi[0]), tokens.bg);
        assert_eq!(paint_color(palette.ansi[7]), tokens.fg);
        assert_eq!(paint_color(palette.ansi[15]), tokens.fg_strong);
    }

    #[test]
    fn the_six_hues_come_from_the_matching_tokens() {
        let tokens = Tokens::one_dark();
        let palette = derived();

        for (index, token) in [
            (1, tokens.red),
            (2, tokens.green),
            (3, tokens.yellow),
            (4, tokens.blue),
            (5, tokens.magenta),
            (6, tokens.cyan),
        ] {
            assert_eq!(paint_color(palette.ansi[index]), token, "ANSI {index}");
        }
    }

    #[test]
    fn every_bright_colour_keeps_its_hue_and_gains_lightness() {
        let palette = derived();

        for index in 1..7 {
            let normal = paint_color(palette.ansi[index]);
            let bright = paint_color(palette.ansi[index + 8]);
            assert!(
                bright.l > normal.l,
                "ANSI {index}: {} should be lighter than {}",
                bright.l,
                normal.l
            );
            assert!(
                (bright.h - normal.h).abs() < 0.01,
                "ANSI {index}: the hue should not move"
            );
        }
        assert!(paint_color(palette.ansi[8]).l > paint_color(palette.ansi[0]).l);
    }

    #[test]
    fn a_theme_sets_the_colours_the_tokens_derived() {
        let theme = ThemeValues {
            background: Some(MARKER),
            palette: BTreeMap::from([(2, MARKER)]),
            ..ThemeValues::default()
        };

        let palette =
            TerminalPalette::resolve(&GhosttyConfig::default(), &theme, &Tokens::one_dark());

        assert_eq!(palette.background, MARKER);
        assert_eq!(palette.ansi[2], MARKER);
        assert_eq!(palette.foreground, derived().foreground);
    }

    #[test]
    fn a_configuration_key_overrides_the_theme_whatever_its_order() {
        let theme = ThemeValues {
            background: Some(MARKER),
            foreground: Some(MARKER),
            ..ThemeValues::default()
        };
        let config = GhosttyConfig {
            background: Some(Rgb { r: 1, g: 2, b: 3 }),
            ..GhosttyConfig::default()
        };

        let palette = TerminalPalette::resolve(&config, &theme, &Tokens::one_dark());

        assert_eq!(palette.background, Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(palette.foreground, MARKER);
    }

    #[test]
    fn a_palette_index_above_the_ansi_colours_leaves_them_alone() {
        let config = GhosttyConfig {
            palette: BTreeMap::from([(200, MARKER)]),
            ..GhosttyConfig::default()
        };

        let palette =
            TerminalPalette::resolve(&config, &ThemeValues::default(), &Tokens::one_dark());

        assert_eq!(palette.ansi, derived().ansi);
    }

    #[test]
    fn a_colour_survives_the_round_trip_through_the_emulator_type() {
        let painted = paint_color(MARKER);

        assert_eq!(painted, to_hsla(crate::palette::to_color(MARKER)));
        assert_eq!(crate::palette::to_color(MARKER).r, MARKER.r);
    }
}
