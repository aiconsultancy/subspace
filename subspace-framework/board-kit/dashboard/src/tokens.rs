//! Design tokens: One Dark with the
//! herdr accent, plus the row and panel sizes. The app's One Dark theme file
//! (`framework/host/app/themes/one-dark.json`) uses the same colours; a test in the
//! app checks that they agree.

use std::collections::BTreeMap;
use std::time::Duration;

use gpui_kit::component::Theme;
use gpui_kit::{App, Global, Hsla, Pixels, px, rgb, rgba};

use crate::density::{
    BODY_FONT_DEFAULT, Density, HEADING_FONT_DEFAULT, MONO_FONT_DEFAULT, TAB_FONT_DEFAULT,
};

mod controls;
mod sizes;
use sizes::{
    CHILD_INDENT_DEFAULT, CHROME_HEIGHT_DEFAULT, COLUMN_HEADER_HEIGHT_DEFAULT,
    CONTROL_RADIUS_DEFAULT, DONE_FRACTION_DEFAULT, HORIZONTAL_PADDING_DEFAULT, LANE_WIDTH_DEFAULT,
    MOTION_MILLIS_DEFAULT, NAV_ROUND_DEFAULT, PANEL_WIDTH_DEFAULT, ROW_HEIGHT_DEFAULT,
    ROW_INSET_DEFAULT, SURFACE_RADIUS_DEFAULT, TOOLBAR_HEIGHT_DEFAULT, TREE_WIDTH_DEFAULT,
};
mod tones;
pub use controls::{ButtonFinish, ButtonMetrics, ButtonRole, Controls, ToolbarLook, rem};
pub use tones::{flat, glass};

const OPAQUE_ALPHA: f32 = 1.;

/// Dashboard-specific semantic colours supplied by theme metadata.
#[derive(Debug, Clone, Default)]
pub struct ThemeTones(pub BTreeMap<String, Hsla>);

impl Global for ThemeTones {}

/// The colours as `0xRRGGBB`, named after the CSS custom properties.
pub mod hex {
    pub const BG: u32 = 0x101115;
    pub const PANE: u32 = 0x15171b;
    pub const LINE: u32 = 0x313640;
    pub const SELECT: u32 = 0x2c313a;
    pub const FG: u32 = 0xabb2bf;
    pub const MUTED: u32 = 0x8b919c;
    pub const ACCENT: u32 = 0x7dd3fc;
    pub const RED: u32 = 0xe06c75;
    pub const ORANGE: u32 = 0xd19a66;
    pub const YELLOW: u32 = 0xe5c07b;
    pub const GREEN: u32 = 0x98c379;
    pub const CYAN: u32 = 0x56b6c2;
    pub const BLUE: u32 = 0x61afef;
    pub const MAGENTA: u32 = 0xc678dd;
    /// `--fg-strong`: `--fg` mixed 60% with white.
    pub const FG_STRONG: u32 = 0xd1d5dc;
    /// `--hair`: `--line` mixed 40% with `--bg`, for row separators.
    pub const HAIR: u32 = 0x1d2026;
}

/// A named colour. Dashboards pick tones; the tokens decide the colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tone {
    Fg,
    Strong,
    Muted,
    Accent,
    Red,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Magenta,
}

/// Colours and sizes for the shared views.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub bg: Hsla,
    pub pane: Hsla,
    pub line: Hsla,
    pub hair: Hsla,
    pub select: Hsla,
    pub fg: Hsla,
    pub fg_strong: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub red: Hsla,
    pub orange: Hsla,
    pub yellow: Hsla,
    pub green: Hsla,
    pub cyan: Hsla,
    pub blue: Hsla,
    pub magenta: Hsla,
    /// The window's top bar and status bar.
    pub chrome: Hsla,
    /// The hairline under the top bar and above the status bar.
    pub chrome_line: Hsla,
    /// Navigation sidebars: the board rail and settings navigation.
    pub sidebar: Hsla,
    pub sidebar_line: Hsla,
    /// The current navigation destination's glass capsule: its translucent
    /// fill, specular rim and lifting shadow. See [`crate::glass`].
    pub glass: Hsla,
    pub glass_rim: Hsla,
    pub glass_shadow: Hsla,
    /// A flat button's fill under the pointer, and a latched one's
    /// persistent fill. See [`crate::icon_button::Surface`].
    pub button_hover: Hsla,
    pub button_on: Hsla,
    /// Raised surfaces: dialogs, menus, popovers and toasts.
    pub surface: Hsla,
    /// Opaque backdrop behind private image previews.
    pub media_stage: Hsla,
    /// `--row-h`
    pub row_height: Pixels,
    /// `--row-font`
    pub row_font: Pixels,
    /// `--heading-font`: reference size for headings.
    pub heading_font: Pixels,
    /// `--tab-font`: reference size for board tab labels.
    pub tab_font: Pixels,
    /// `--mono-font`: reference size for code and logs.
    pub mono_font: Pixels,
    /// `--pad-x`
    pub pad_x: Pixels,
    /// `--indent`: chevron slot plus gap, so child icons line up.
    pub indent: Pixels,
    /// `--row-inset`: 0 draws full-width rows with hairline separators; more
    /// draws each row's hover and selection as a rounded fill inset by this much.
    pub row_inset: Pixels,
    /// `--chrome-h`: the window's top bar.
    pub chrome_height: Pixels,
    /// `--toolbar-h`, also the detail panel's bar.
    pub toolbar_height: Pixels,
    /// `--panel-w`
    pub panel_width: Pixels,
    /// `--head-h`: the board's column header.
    pub head_height: Pixels,
    /// `--lane-w`: the board's Parent column.
    pub lane_width: Pixels,
    /// `--done-w`: the board's Done column, in fractions of a column.
    pub done_fraction: f32,
    /// `--tree-w`: the diff view's file tree.
    pub tree_width: Pixels,
    /// `--motion`
    pub motion: Duration,
    /// Controls such as buttons, rows, chips and inputs.
    pub control_radius: Pixels,
    /// Surfaces such as dialogs, menus, toasts, cards and feedback banners.
    pub surface_radius: Pixels,
    /// `--nav-round`: navigation capsule corners as a fraction of half the
    /// item's height; 1 is a full capsule.
    pub nav_round: f32,
    /// Button geometry, dialog spacing and the control style.
    pub controls: Controls,
}

impl Tokens {
    pub fn one_dark() -> Self {
        let color = |hex: u32| Hsla::from(rgb(hex));
        Self {
            bg: color(hex::BG),
            pane: color(hex::PANE),
            line: color(hex::LINE),
            hair: color(hex::HAIR),
            select: color(hex::SELECT),
            fg: color(hex::FG),
            fg_strong: color(hex::FG_STRONG),
            muted: color(hex::MUTED),
            accent: color(hex::ACCENT),
            red: color(hex::RED),
            orange: color(hex::ORANGE),
            yellow: color(hex::YELLOW),
            green: color(hex::GREEN),
            cyan: color(hex::CYAN),
            blue: color(hex::BLUE),
            magenta: color(hex::MAGENTA),
            chrome: color(hex::BG),
            chrome_line: color(hex::LINE),
            sidebar: color(hex::BG),
            sidebar_line: color(hex::LINE),
            glass: Hsla::from(rgba(glass::DARK_FILL)),
            glass_rim: Hsla::from(rgba(glass::DARK_RIM)),
            glass_shadow: Hsla::from(rgba(glass::DARK_SHADOW)),
            button_hover: Hsla::from(rgba(flat::DARK_HOVER)),
            button_on: color(hex::ACCENT).opacity(flat::DARK_ON),
            surface: color(hex::PANE),
            media_stage: color(hex::BG),
            row_height: px(ROW_HEIGHT_DEFAULT),
            row_font: px(BODY_FONT_DEFAULT),
            heading_font: px(HEADING_FONT_DEFAULT),
            tab_font: px(TAB_FONT_DEFAULT),
            mono_font: px(MONO_FONT_DEFAULT),
            pad_x: px(HORIZONTAL_PADDING_DEFAULT),
            indent: px(CHILD_INDENT_DEFAULT),
            row_inset: px(ROW_INSET_DEFAULT),
            chrome_height: px(CHROME_HEIGHT_DEFAULT),
            toolbar_height: px(TOOLBAR_HEIGHT_DEFAULT),
            panel_width: px(PANEL_WIDTH_DEFAULT),
            head_height: px(COLUMN_HEADER_HEIGHT_DEFAULT),
            lane_width: px(LANE_WIDTH_DEFAULT),
            done_fraction: DONE_FRACTION_DEFAULT,
            tree_width: px(TREE_WIDTH_DEFAULT),
            motion: Duration::from_millis(MOTION_MILLIS_DEFAULT),
            control_radius: px(CONTROL_RADIUS_DEFAULT),
            surface_radius: px(SURFACE_RADIUS_DEFAULT),
            nav_round: NAV_ROUND_DEFAULT,
            controls: Default::default(),
        }
    }

    /// One Dark with the density jig's values (REQ-10).
    pub fn with_density(density: &Density) -> Self {
        let size = |token: &str| px(density.get(token));
        Self {
            row_height: size("--row-h"),
            row_font: size("--row-font"),
            heading_font: size("--heading-font"),
            tab_font: size("--tab-font"),
            mono_font: size("--mono-font"),
            pad_x: size("--pad-x"),
            indent: size("--indent"),
            row_inset: size("--row-inset"),
            chrome_height: size("--chrome-h"),
            toolbar_height: size("--toolbar-h"),
            panel_width: size("--panel-w"),
            head_height: size("--head-h"),
            lane_width: size("--lane-w"),
            done_fraction: density.get("--done-w"),
            tree_width: size("--tree-w"),
            motion: Duration::from_millis(density.get("--motion").max(0.0) as u64),
            control_radius: size("--radius"),
            surface_radius: size("--surface-radius"),
            nav_round: density.get("--nav-round"),
            controls: Controls::from_density(density),
            ..Self::one_dark()
        }
    }

    /// True when rows draw inset rounded fills instead of separators.
    pub fn inset_rows(&self) -> bool {
        self.row_inset > Pixels::ZERO
    }

    /// The tokens in effect: One Dark with the [`Density`] global, when the
    /// app has set one.
    pub fn current(cx: &App) -> Self {
        let tokens = match cx.try_global::<Density>() {
            Some(density) => Self::with_density(density),
            None => Self::one_dark(),
        };
        let tokens = match cx.try_global::<Theme>() {
            Some(theme) => Self::with_theme(tokens, theme),
            None => tokens,
        };
        match cx.try_global::<ThemeTones>() {
            Some(tones) => Self::with_tones(tokens, tones),
            None => tokens,
        }
    }

    fn with_theme(mut tokens: Self, theme: &Theme) -> Self {
        tokens.bg = theme.background;
        tokens.pane = theme.muted;
        tokens.line = theme.border;
        tokens.hair = theme.list_hover;
        tokens.select = theme.accent;
        tokens.fg = theme.foreground;
        tokens.fg_strong = theme.caret;
        tokens.muted = theme.muted_foreground;
        tokens.accent = theme.primary;
        tokens.red = theme.danger;
        // GPUI Kit has no orange semantic token. Keep Subspace's exact
        // service colour rather than deriving a different value.
        tokens.yellow = theme.warning;
        tokens.green = theme.success;
        tokens.cyan = theme.info;
        tokens.blue = theme.blue;
        tokens.magenta = theme.magenta;
        tokens.chrome = theme.title_bar;
        tokens.chrome_line = theme.title_bar_border;
        tokens.sidebar = theme.sidebar;
        tokens.sidebar_line = theme.sidebar_border;
        tokens.surface = theme.popover;
        tokens.media_stage = theme.background;
        tokens.media_stage.a = OPAQUE_ALPHA;
        let ([fill, rim, shadow, hover], on) = if theme.mode.is_dark() {
            (
                [
                    glass::DARK_FILL,
                    glass::DARK_RIM,
                    glass::DARK_SHADOW,
                    flat::DARK_HOVER,
                ],
                flat::DARK_ON,
            )
        } else {
            (
                [
                    glass::LIGHT_FILL,
                    glass::LIGHT_RIM,
                    glass::LIGHT_SHADOW,
                    flat::LIGHT_HOVER,
                ],
                flat::LIGHT_ON,
            )
        };
        tokens.glass = Hsla::from(rgba(fill));
        tokens.glass_rim = Hsla::from(rgba(rim));
        tokens.glass_shadow = Hsla::from(rgba(shadow));
        tokens.button_hover = Hsla::from(rgba(hover));
        tokens.button_on = theme.primary.opacity(on);
        tokens
    }

    fn with_tones(mut tokens: Self, tones: &ThemeTones) -> Self {
        for (name, target) in [
            ("accent", &mut tokens.accent),
            ("red", &mut tokens.red),
            ("green", &mut tokens.green),
            ("yellow", &mut tokens.yellow),
            ("blue", &mut tokens.blue),
            ("cyan", &mut tokens.cyan),
            ("magenta", &mut tokens.magenta),
            ("orange", &mut tokens.orange),
            ("glass", &mut tokens.glass),
            ("glass-rim", &mut tokens.glass_rim),
            ("glass-shadow", &mut tokens.glass_shadow),
            ("button-hover", &mut tokens.button_hover),
            ("button-on", &mut tokens.button_on),
            ("media-stage", &mut tokens.media_stage),
        ] {
            if let Some(color) = tones.0.get(name) {
                *target = *color;
            }
        }
        tokens
    }

    /// A glyph's colour: its exact colour when it has one, else its tone.
    pub fn glyph(&self, glyph: &crate::list::Glyph) -> Hsla {
        match glyph.rgb {
            Some(hex) => Hsla::from(rgb(hex)),
            None => self.tone(glyph.tone),
        }
    }

    pub fn tone(&self, tone: Tone) -> Hsla {
        match tone {
            Tone::Fg => self.fg,
            Tone::Strong => self.fg_strong,
            Tone::Muted => self.muted,
            Tone::Accent => self.accent,
            Tone::Red => self.red,
            Tone::Orange => self.orange,
            Tone::Yellow => self.yellow,
            Tone::Green => self.green,
            Tone::Cyan => self.cyan,
            Tone::Blue => self.blue,
            Tone::Magenta => self.magenta,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ThemeTones, Tokens, glass, hex};
    use crate::density::Density;
    use crate::list::Glyph;
    use crate::tokens::Tone;
    use gpui_kit::component::{Theme, ThemeMode};
    use gpui_kit::{Hsla, px, rgb, rgba};
    use std::collections::BTreeMap;

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn media_stage_fallback_is_opaque_with_a_translucent_custom_theme() {
        let mut theme = Theme::default();
        theme.background = Hsla::from(rgba(0x10203044));
        let tokens = Tokens::with_theme(Tokens::one_dark(), &theme);
        let mut opaque = theme.background;
        opaque.a = 1.;
        assert_eq!(tokens.media_stage, opaque);
        assert_eq!(tokens.bg, theme.background);
    }

    #[test]
    fn one_dark_keeps_the_reference_palette() {
        let tokens = Tokens::one_dark();
        assert_eq!(tokens.bg, Hsla::from(rgb(hex::BG)));
        assert_eq!(tokens.pane, Hsla::from(rgb(hex::PANE)));
        assert_eq!(tokens.line, Hsla::from(rgb(hex::LINE)));
        assert_eq!(tokens.hair, Hsla::from(rgb(hex::HAIR)));
        assert_eq!(tokens.select, Hsla::from(rgb(hex::SELECT)));
        assert_eq!(tokens.fg, Hsla::from(rgb(hex::FG)));
        assert_eq!(tokens.fg_strong, Hsla::from(rgb(hex::FG_STRONG)));
        assert_eq!(tokens.muted, Hsla::from(rgb(hex::MUTED)));
        assert_eq!(tokens.accent, Hsla::from(rgb(hex::ACCENT)));
        assert_eq!(tokens.orange, Hsla::from(rgb(hex::ORANGE)));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn active_theme_mapping_keeps_density_and_exact_glyph_rgb() {
        let mut density = Density::default();
        density.set("--row-h", 42.);
        let base = Tokens::with_density(&density);
        let mut theme = Theme::default();
        theme.background = Hsla::from(rgb(0xfafafa));
        theme.foreground = Hsla::from(rgb(0x202020));
        theme.primary = Hsla::from(rgb(0x0066cc));
        let tokens = Tokens::with_theme(base, &theme);

        assert_eq!(tokens.bg, Hsla::from(rgb(0xfafafa)));
        assert_eq!(tokens.fg, Hsla::from(rgb(0x202020)));
        assert_eq!(tokens.row_height, px(42.));
        let glyph = Glyph::new("label", Tone::Red);
        let exact = Glyph {
            rgb: Some(0x123456),
            ..glyph
        };
        assert_eq!(tokens.glyph(&exact), Hsla::from(rgb(0x123456)));
    }

    #[test]
    fn theme_tones_overlay_only_known_dashboard_colours() {
        let tones = ThemeTones(BTreeMap::from([
            ("accent".into(), Hsla::from(rgb(0x007fa2))),
            ("glass".into(), Hsla::from(rgba(0xffffff33))),
            ("unknown".into(), Hsla::from(rgb(0x123456))),
        ]));
        let tokens = Tokens::with_tones(Tokens::one_dark(), &tones);

        assert_eq!(tokens.accent, Hsla::from(rgb(0x007fa2)));
        assert_eq!(tokens.glass, Hsla::from(rgba(0xffffff33)));
        assert_eq!(tokens.glass_rim, Hsla::from(rgba(glass::DARK_RIM)));
        assert_eq!(tokens.red, Hsla::from(rgb(hex::RED)));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn glass_follows_the_theme_mode_and_the_capsule_is_a_density_token() {
        let mut light = Theme::default();
        light.mode = ThemeMode::Light;
        let mut dark = Theme::default();
        dark.mode = ThemeMode::Dark;
        let base = Tokens::with_density(&Density::default());
        let light = Tokens::with_theme(base, &light);
        let dark = Tokens::with_theme(base, &dark);
        assert_eq!(light.glass, Hsla::from(rgba(glass::LIGHT_FILL)));
        assert_eq!(light.glass_shadow, Hsla::from(rgba(glass::LIGHT_SHADOW)));
        assert_eq!(dark.glass, Hsla::from(rgba(glass::DARK_FILL)));
        assert!(dark.glass.a < 0.2, "dark glass stays translucent");
        assert_eq!(base.nav_round, 1.);
        assert_eq!(
            Tokens::with_density(&Density::from_json(r#"{"--nav-round":"0.5fr"}"#)).nav_round,
            0.5
        );
    }

    #[test]
    fn font_size_roles_preserve_defaults_and_scale_independently() {
        let defaults = Tokens::with_density(&Density::default());
        assert_eq!(defaults.heading_size(16.), px(16.));
        assert_eq!(defaults.tab_size(15.), px(15.));
        assert_eq!(defaults.mono_size(12.5), px(12.5));
        let density = Density::from_json(
            r#"{"--heading-font":"21px","--row-font":"16px","--mono-font":"19.5px"}"#,
        );
        let tokens = Tokens::with_density(&density);
        assert_eq!(tokens.heading_size(15.), px(21.));
        assert_eq!(
            tokens.tab_size(15.),
            px(15.),
            "tabs no longer follow headings"
        );
        let tabs = Tokens::with_density(&Density::from_json(r#"{"--tab-font":"12px"}"#));
        assert_eq!(tabs.tab_size(15.), px(12.));
        assert_eq!(
            tabs.heading_size(15.),
            px(15.),
            "headings do not follow tabs"
        );
        assert_eq!(tokens.mono_size(12.), px(18.));
        assert_eq!(tokens.row_font, px(16.));
        assert_eq!(defaults.caption_size(), px(11.));
        assert_eq!(defaults.secondary_size(), px(12.));
        assert_eq!(tokens.caption_size(), px(16. * 11. / 13.));
        assert_eq!(tokens.secondary_size(), px(16. * 12. / 13.));
        assert_eq!(tokens.control_radius, px(4.));
        assert_eq!(tokens.surface_radius, px(6.));
    }
}
