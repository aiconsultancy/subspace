//! Everything a session needs to draw itself, resolved once (REQ-12, REQ-14
//! to REQ-17).
//!
//! The board builds one [`TerminalStyle`] from the Ghostty configuration and
//! the resolved theme and hands it to every view it opens, so a view never
//! reads a configuration file or a GPUI global of its own. On a reload or an
//! appearance change the board builds a new style and calls
//! [`crate::TerminalView::apply_config`], which re-measures the grid and resizes
//! the session (REQ-14, REQ-17, REQ-33).

use std::collections::BTreeMap;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, FontStyle, FontWeight, Pixels, SharedString, px};
use libghostty_vt::Terminal;
use libghostty_vt::terminal::CursorStyle as EmulatorCursorStyle;
use subspace_framework::dashboard::tokens::Tokens;
use subspace_framework::dashboard::typography;
use subspace_ghostty_config::{AdjustCellHeight, CursorStyle, GhosttyConfig, Rgb, ThemeValues};

use crate::clipboard::TerminalClipboard;
use crate::error::{SessionError, emulator_error};
use crate::input::TerminalKeybinds;
use crate::metrics::Padding;
use crate::palette::{self, TerminalPalette};

mod profile;

pub use profile::ProfileFont;

/// The families a session draws with, one per combination of weight and
/// slant, already matched against the installed families (REQ-14).
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalFonts {
    /// `font-family`.
    pub regular: SharedString,
    /// `font-family-bold`.
    pub bold: SharedString,
    /// `font-family-italic`.
    pub italic: SharedString,
    /// `font-family-bold-italic`.
    pub bold_italic: SharedString,
    pub(crate) base_weight: FontWeight,
    pub(crate) base_style: FontStyle,
    /// `font-size`, or the appearance code size.
    pub size: Pixels,
    /// `adjust-cell-height`.
    pub cell_height: Option<AdjustCellHeight>,
}

/// A family and size that may each inherit independently.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontChoice {
    pub family: Option<String>,
    pub size: Option<f32>,
}

/// The cursor a session starts with, before the program asks for another
/// shape (REQ-15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorDefaults {
    /// `cursor-style`.
    pub style: CursorStyle,
    /// `cursor-style-blink`.
    pub blink: bool,
}

/// How a session draws: its colours, its fonts, its cursor, its padding and
/// its scroll rate.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalStyle {
    /// The colours, Ghostty's over the Subspace theme's (REQ-12).
    pub palette: TerminalPalette,
    /// The palette entries the configuration and the theme name, which the
    /// emulator resolves indexed colours through (REQ-12).
    pub indexed: BTreeMap<u8, Rgb>,
    /// The fonts and the cell height (REQ-14).
    pub fonts: TerminalFonts,
    /// The cursor the session starts with (REQ-15).
    pub cursor: CursorDefaults,
    /// `window-padding-x` and `window-padding-y`, kept inside the bounds
    /// (REQ-17).
    pub padding: Padding,
    /// `mouse-scroll-multiplier`, applied to the wheel (REQ-16).
    pub mouse_scroll_multiplier: f32,
    /// The emulator's maximum scrollback size in bytes (REQ-16, REQ-33).
    pub scrollback_limit_bytes: usize,
    /// The `keybind` lines that send bytes, checked before the key encoder
    /// (REQ-31a).
    pub keybinds: TerminalKeybinds,
    /// The clipboard policy: copy-on-select, paste protection and what a
    /// program's own clipboard sequences may do (REQ-22 to REQ-24).
    pub clipboard: TerminalClipboard,
}

/// The appearance values a style falls back to where Ghostty sets nothing:
/// the code font family and size, and the families the host has installed.
struct AppearanceFonts {
    /// The appearance code font family.
    code_family: String,
    /// The appearance code font size, in points.
    code_size: Pixels,
    /// Every family the text system reports.
    installed: Vec<String>,
}

/// One font role's request: the Ghostty families in order, and what to use
/// when none of them is installed.
struct FamilyRequest<'a> {
    /// The role the warning names.
    role: &'a str,
    /// The families the configuration asked for, in order.
    families: &'a [String],
    /// The family to use when none of them is installed.
    fallback: &'a str,
}

impl TerminalStyle {
    /// The style for a session: Ghostty's values over the active appearance.
    ///
    /// The warnings name each requested font family that the host does not
    /// have, so the board can show them beside the configuration warnings
    /// (REQ-32).
    pub fn resolve(config: &GhosttyConfig, theme: &ThemeValues, cx: &App) -> (Self, Vec<String>) {
        Self::resolve_with_fonts(
            config,
            theme,
            &FontChoice::default(),
            &ProfileFont::default(),
            cx,
        )
    }

    /// Resolve explicit Subspace choices, then Ghostty, the platform profile and Appearance.
    pub fn resolve_with_fonts(
        config: &GhosttyConfig,
        theme: &ThemeValues,
        choice: &FontChoice,
        profile: &ProfileFont,
        cx: &App,
    ) -> (Self, Vec<String>) {
        let appearance = appearance_fonts(cx);
        let profile = profile.validated(&appearance.installed, cx.text_system());
        let mut warnings = Vec::new();
        let style = Self {
            palette: TerminalPalette::resolve(config, theme, &Tokens::current(cx)),
            indexed: indexed_entries(config, theme),
            fonts: resolve_fonts(config, choice, &profile, &appearance, &mut warnings),
            cursor: CursorDefaults {
                style: config.cursor_style,
                blink: config.cursor_style_blink,
            },
            padding: Padding {
                x: config.window_padding_x,
                y: config.window_padding_y,
            },
            mouse_scroll_multiplier: config.mouse_scroll_multiplier,
            scrollback_limit_bytes: usize::try_from(config.scrollback_limit)
                .unwrap_or(crate::command::DEFAULT_SCROLLBACK_LIMIT_BYTES),
            keybinds: TerminalKeybinds::parse(&config.keybind),
            clipboard: TerminalClipboard::from_config(config),
        };
        (style, warnings)
    }
}

/// Give the emulator the parts of a style it owns itself: the palette, the
/// default colours and the cursor the program resets to (REQ-12, REQ-15).
pub(crate) fn apply_to_terminal(
    style: &TerminalStyle,
    terminal: &mut Terminal<'static, 'static>,
) -> Result<(), SessionError> {
    palette::apply_to_terminal(&style.palette, &style.indexed, terminal)?;
    terminal
        .set_default_cursor_style(Some(emulator_cursor_style(style.cursor.style)))
        .map_err(emulator_error)?;
    terminal
        .set_default_cursor_blink(Some(style.cursor.blink))
        .map_err(emulator_error)?;
    terminal
        .set_scrollback_max_bytes(Some(style.scrollback_limit_bytes))
        .map_err(emulator_error)?;
    Ok(())
}

/// The palette entries the theme and then the configuration name, in one
/// table: the configuration wins on an index both set (REQ-30).
fn indexed_entries(config: &GhosttyConfig, theme: &ThemeValues) -> BTreeMap<u8, Rgb> {
    let mut entries = theme.palette.clone();
    entries.extend(config.palette.iter().map(|(index, color)| (*index, *color)));
    entries
}

/// The appearance code font and the installed families (REQ-14).
fn appearance_fonts(cx: &App) -> AppearanceFonts {
    AppearanceFonts {
        code_family: cx.theme().mono_font_family.to_string(),
        code_size: Tokens::current(cx).mono_font,
        installed: cx.text_system().all_font_names(),
    }
}

/// The families and sizes a session draws with (REQ-14).
///
/// Each role falls back to the next listed family, then to the family the
/// regular role resolved to, and finally to the appearance code font, so a
/// configuration that sets only `font-family` keeps one family throughout.
fn resolve_fonts(
    config: &GhosttyConfig,
    choice: &FontChoice,
    profile: &ProfileFont,
    appearance: &AppearanceFonts,
    warnings: &mut Vec<String>,
) -> TerminalFonts {
    let ghostty = typography::first_installed_family(&config.font_family, &appearance.installed);
    let explicit = resolve_choice(choice, appearance, warnings);
    let use_profile = explicit.is_none() && ghostty.is_none() && profile.family.is_some();
    // When a configured family cannot load, use the platform profile as a
    // whole. A size on its own still overrides the inherited profile size.
    let ghostty_size = config
        .font_size
        .filter(|size| size.is_finite() && *size > 0.0);
    let size = choice
        .size
        .or(if use_profile && !config.font_family.is_empty() {
            profile.size.or(ghostty_size)
        } else {
            ghostty_size.or(profile.size)
        })
        .map_or(appearance.code_size, px);
    let [regular, bold, italic, bold_italic] = match explicit {
        Some(family) => {
            let family: SharedString = family.into();
            std::array::from_fn(|_| family.clone())
        }
        None => {
            let fallback = profile.family.as_deref().unwrap_or(&appearance.code_family);
            let regular = ghostty.unwrap_or_else(|| {
                FamilyRequest {
                    role: "Terminal font",
                    families: &config.font_family,
                    fallback,
                }
                .fall_back(&appearance.installed, warnings)
            });
            configured_families(config, &regular, appearance, warnings)
        }
    };
    TerminalFonts {
        regular,
        bold,
        italic,
        bold_italic,
        base_weight: if use_profile {
            profile.weight
        } else {
            FontWeight::NORMAL
        },
        base_style: if use_profile {
            profile.style
        } else {
            FontStyle::Normal
        },
        size,
        cell_height: config.adjust_cell_height,
    }
}

fn configured_families(
    config: &GhosttyConfig,
    regular: &str,
    appearance: &AppearanceFonts,
    warnings: &mut Vec<String>,
) -> [SharedString; 4] {
    [
        regular.to_owned().into(),
        styled_family(
            "Terminal bold font",
            &config.font_family_bold,
            regular,
            appearance,
            warnings,
        ),
        styled_family(
            "Terminal italic font",
            &config.font_family_italic,
            regular,
            appearance,
            warnings,
        ),
        styled_family(
            "Terminal bold italic font",
            &config.font_family_bold_italic,
            regular,
            appearance,
            warnings,
        ),
    ]
}

fn installed_choice(choice: &FontChoice, appearance: &AppearanceFonts) -> Option<String> {
    typography::installed_family(choice.family.as_deref()?, &appearance.installed)
}

fn resolve_choice(
    choice: &FontChoice,
    appearance: &AppearanceFonts,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let requested = choice.family.as_ref()?;
    let installed = installed_choice(choice, appearance);
    if installed.is_none() {
        warnings.push(format!(
            "Terminal font: {requested} is not installed; using Automatic"
        ));
    }
    installed
}

/// One of the bold, italic or bold-italic families, falling back to the
/// family the regular role resolved to.
fn styled_family(
    role: &str,
    families: &[String],
    regular: &str,
    appearance: &AppearanceFonts,
    warnings: &mut Vec<String>,
) -> SharedString {
    resolve_family(
        &FamilyRequest {
            role,
            families,
            fallback: regular,
        },
        &appearance.installed,
        warnings,
    )
    .into()
}

/// The first installed family of a role's list, else its fallback.
fn resolve_family(
    request: &FamilyRequest<'_>,
    installed: &[String],
    warnings: &mut Vec<String>,
) -> String {
    typography::first_installed_family(request.families, installed)
        .unwrap_or_else(|| request.fall_back(installed, warnings))
}

impl FamilyRequest<'_> {
    /// The fallback family, with the warning the board shows when a family
    /// was asked for and none of the list is installed (REQ-14, REQ-32).
    fn fall_back(&self, installed: &[String], warnings: &mut Vec<String>) -> String {
        typography::resolve_role(
            self.role,
            self.requested(),
            self.fallback,
            installed,
            warnings,
        )
    }

    /// The family the warning names: the first the configuration asked for,
    /// and nothing when the role is unset.
    fn requested(&self) -> &str {
        match self.families.first() {
            Some(family) => family,
            None => "",
        }
    }
}

/// A configured cursor style as the emulator's own (REQ-15).
// qual:allow(dry, boilerplate) reason: "Two crates name the same four cursor
// shapes and neither can implement a conversion for the other's type, so the
// four arms are the whole mapping."
fn emulator_cursor_style(style: CursorStyle) -> EmulatorCursorStyle {
    match style {
        CursorStyle::Block => EmulatorCursorStyle::Block,
        CursorStyle::Bar => EmulatorCursorStyle::Bar,
        CursorStyle::Underline => EmulatorCursorStyle::Underline,
        CursorStyle::BlockHollow => EmulatorCursorStyle::BlockHollow,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use gpui_kit::{FontWeight, px};
    use subspace_ghostty_config::{GhosttyConfig, Rgb, ThemeValues};

    use crate::style::{
        AppearanceFonts, FontChoice, ProfileFont, TerminalFonts, indexed_entries,
        resolve_fonts as with_font_choices,
    };

    fn resolve_fonts(
        config: &GhosttyConfig,
        appearance: &AppearanceFonts,
        warnings: &mut Vec<String>,
    ) -> TerminalFonts {
        with_font_choices(
            config,
            &FontChoice::default(),
            &ProfileFont::default(),
            appearance,
            warnings,
        )
    }

    /// The appearance a test falls back to.
    fn appearance(installed: &[&str]) -> AppearanceFonts {
        AppearanceFonts {
            code_family: "Menlo".to_owned(),
            code_size: px(12.5),
            installed: installed.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    /// A configuration with one font-family list.
    fn with_families(families: &[&str]) -> GhosttyConfig {
        GhosttyConfig {
            font_family: families.iter().map(|name| (*name).to_owned()).collect(),
            ..GhosttyConfig::default()
        }
    }

    #[test]
    fn subspace_ghostty_and_platform_values_inherit_independently() {
        let profile = ProfileFont {
            family: Some("Monaco".to_string()),
            size: Some(17.0),
            ..ProfileFont::default()
        };
        let appearance = appearance(&["Menlo", "Monaco", "Courier New"]);
        let config = GhosttyConfig {
            font_family_bold: vec!["Menlo".to_owned()],
            font_size: Some(11.0),
            ..GhosttyConfig::default()
        };
        let mut warnings = Vec::new();
        let automatic = with_font_choices(
            &config,
            &FontChoice::default(),
            &profile,
            &appearance,
            &mut warnings,
        );
        assert_eq!(automatic.regular, "Monaco");
        assert_eq!(automatic.bold, "Menlo");
        assert_eq!(automatic.size, px(11.0));
        let choice = FontChoice {
            family: Some("courier new".to_owned()),
            size: None,
        };
        let selected = with_font_choices(&config, &choice, &profile, &appearance, &mut warnings);
        assert_eq!(selected.regular, "Courier New");
        assert_eq!(selected.bold, "Courier New");
        assert_eq!(selected.italic, "Courier New");
        assert_eq!(selected.size, px(11.0));
        assert!(warnings.is_empty());
    }

    #[test]
    fn an_unavailable_ghostty_family_uses_the_profile_face_and_size() {
        let profile = ProfileFont {
            family: Some("Fixture Nerd Font".to_owned()),
            size: Some(18.0),
            weight: FontWeight::SEMIBOLD,
            ..ProfileFont::default()
        };
        let config = GhosttyConfig {
            font_size: Some(16.0),
            ..with_families(&["Missing"])
        };
        let appearance = appearance(&["Menlo", "Fixture Nerd Font"]);
        let mut warnings = Vec::new();
        let fonts = with_font_choices(
            &config,
            &FontChoice::default(),
            &profile,
            &appearance,
            &mut warnings,
        );
        assert_eq!(fonts.regular, "Fixture Nerd Font");
        assert_eq!(fonts.base_weight, FontWeight::SEMIBOLD);
        assert_eq!(fonts.size, px(18.0));
        assert!(warnings[0].contains("Missing"));

        let available = GhosttyConfig {
            font_family: vec!["Menlo".to_owned()],
            ..config
        };
        let fonts = with_font_choices(
            &available,
            &FontChoice::default(),
            &profile,
            &appearance,
            &mut warnings,
        );
        assert_eq!(fonts.regular, "Menlo");
        assert_eq!(fonts.base_weight, FontWeight::NORMAL);
        assert_eq!(fonts.size, px(16.0));
    }

    #[test]
    fn missing_explicit_family_inherits_and_keeps_the_explicit_size() {
        let profile = ProfileFont {
            family: Some("Monaco".to_string()),
            size: Some(17.0),
            ..ProfileFont::default()
        };
        let choice = FontChoice {
            family: Some("Uninstalled".to_owned()),
            size: Some(20.0),
        };
        let mut warnings = Vec::new();
        let selected = with_font_choices(
            &GhosttyConfig::default(),
            &choice,
            &profile,
            &appearance(&["Menlo", "Monaco"]),
            &mut warnings,
        );
        assert_eq!(selected.regular, "Monaco");
        assert_eq!(selected.size, px(20.0));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn an_unset_font_takes_the_appearance_code_font_and_size() {
        let mut warnings = Vec::new();

        let fonts = resolve_fonts(
            &GhosttyConfig::default(),
            &appearance(&["Menlo"]),
            &mut warnings,
        );

        assert_eq!(fonts.regular, "Menlo");
        assert_eq!(fonts.bold, "Menlo");
        assert_eq!(fonts.size, px(12.5));
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_configured_family_and_size_are_used_as_given() {
        let config = GhosttyConfig {
            font_size: Some(13.0),
            ..with_families(&["JetBrains Mono"])
        };
        let mut warnings = Vec::new();

        let fonts = resolve_fonts(
            &config,
            &appearance(&["Menlo", "JetBrains Mono"]),
            &mut warnings,
        );

        assert_eq!(fonts.regular, "JetBrains Mono");
        assert_eq!(fonts.size, px(13.0));
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_missing_family_takes_the_next_listed_one_and_warns_only_at_the_end() {
        let mut listed = Vec::new();
        let mut alone = Vec::new();

        let next = resolve_fonts(
            &with_families(&["Missing Mono", "Menlo"]),
            &appearance(&["Menlo"]),
            &mut listed,
        );
        let code = resolve_fonts(
            &with_families(&["Missing Mono"]),
            &appearance(&["Menlo"]),
            &mut alone,
        );

        assert_eq!(next.regular, "Menlo", "the next listed family is installed");
        assert!(listed.is_empty(), "{listed:?}");
        assert_eq!(code.regular, "Menlo", "the appearance code font is left");
        assert_eq!(alone.len(), 1);
        assert!(alone[0].contains("Missing Mono"), "{:?}", alone[0]);
    }

    #[test]
    fn the_styled_roles_follow_the_resolved_regular_family() {
        let config = GhosttyConfig {
            font_family_italic: vec!["Missing Italic".to_owned()],
            ..with_families(&["JetBrains Mono"])
        };
        let mut warnings = Vec::new();

        let fonts = resolve_fonts(
            &config,
            &appearance(&["Menlo", "JetBrains Mono"]),
            &mut warnings,
        );

        assert_eq!(fonts.bold, "JetBrains Mono");
        assert_eq!(fonts.bold_italic, "JetBrains Mono");
        assert_eq!(fonts.italic, "JetBrains Mono");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("italic"), "{:?}", warnings[0]);
    }

    #[test]
    fn a_configuration_palette_entry_wins_over_the_themes() {
        let marker = Rgb { r: 1, g: 2, b: 3 };
        let theme = ThemeValues {
            palette: BTreeMap::from([(1, Rgb { r: 9, g: 9, b: 9 }), (2, marker)]),
            ..ThemeValues::default()
        };
        let config = GhosttyConfig {
            palette: BTreeMap::from([(1, marker)]),
            ..GhosttyConfig::default()
        };

        let entries = indexed_entries(&config, &theme);

        assert_eq!(entries.get(&1), Some(&marker));
        assert_eq!(entries.get(&2), Some(&marker));
    }
}
