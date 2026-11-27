//! Theme and font registration for the application.

mod document;
mod fonts;
mod loader;
mod new_install;
#[cfg(test)]
pub(crate) mod preview_tests;
pub(crate) mod storage;
#[cfg(test)]
mod switch_tests;

use std::collections::BTreeMap;

use gpui_kit::component::{Theme, try_parse_color};
use gpui_kit::{App, Styled as _, px};
use loader::{ThemeChoices, default_theme, load_registered_themes};
use subspace_dashboard::density::{self, BODY_FONT_DEFAULT, Density, DensityPreview};
use subspace_dashboard::prefs;
use subspace_dashboard::tokens::{ThemeId, ThemeTones};
use subspace_dashboard::typography::{GroupHeading, Typography};

pub(crate) use document::{FontChoices, GroupHeadingChoices, KIT_RADII, ThemeDocument};

pub use loader::ProductThemes;
pub(crate) use loader::bundled_theme_id;
pub(crate) use loader::validate_custom_theme;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use loader::validate_product_theme_documents;
pub(crate) use loader::{install_product_themes, product_themes, validate_product_themes};
pub use new_install::record_new_install_theme;

pub const THEME_JSON: &str = include_str!("../themes/one-dark.json");
/// Native macOS library materials in light and dark variants.
pub const CUPERTINO_JSON: &str = include_str!("../themes/cupertino.json");
/// Subspace Channel: a blue-black canvas with warm graphite surfaces, and its
/// light counterpart.
pub const SUBSPACE_CHANNEL_JSON: &str = include_str!("../themes/subspace-channel.json");
/// huacnlee's macOS Classic (MIT, `licenses/themes/`), remixed with Subspace
/// sizes.
pub const MACOS_CLASSIC_DARK_REMIX_JSON: &str =
    include_str!("../themes/macos-classic-dark-remix.json");
/// Twilight Two: the workspace shell mock-ups' colours and sizes, with
/// Twilight's syntax colours (Apache-2.0, `themes/gpui-kit/`).
pub const TWILIGHT_TWO_JSON: &str = include_str!("../themes/twilight-two.json");
/// Subspace's own themes by product theme id, ahead of the GPUI Kit catalogue.
pub const SUBSPACE_THEMES: &[(&str, &str)] = &[
    ("one-dark", THEME_JSON),
    ("cupertino", CUPERTINO_JSON),
    ("subspace-channel", SUBSPACE_CHANNEL_JSON),
    ("macos-classic-dark-remix", MACOS_CLASSIC_DARK_REMIX_JSON),
    ("twilight-two", TWILIGHT_TWO_JSON),
];
#[cfg(test)]
pub const THEME_NAME: &str = "Subspace One Dark";

/// The complete GPUI Kit 0.11.0 theme catalogue, vendored from the matching
/// `gpui-kit` release and embedded so startup never depends on the filesystem.
pub const GPUI_KIT_THEMES: &[(&str, &str)] = &[
    (
        "adventure",
        include_str!("../themes/gpui-kit/adventure.json"),
    ),
    ("alduin", include_str!("../themes/gpui-kit/alduin.json")),
    (
        "asciinema",
        include_str!("../themes/gpui-kit/asciinema.json"),
    ),
    ("aurora", include_str!("../themes/gpui-kit/aurora.json")),
    ("ayu", include_str!("../themes/gpui-kit/ayu.json")),
    (
        "catppuccin",
        include_str!("../themes/gpui-kit/catppuccin.json"),
    ),
    (
        "everforest",
        include_str!("../themes/gpui-kit/everforest.json"),
    ),
    (
        "fahrenheit",
        include_str!("../themes/gpui-kit/fahrenheit.json"),
    ),
    ("flexoki", include_str!("../themes/gpui-kit/flexoki.json")),
    ("gruvbox", include_str!("../themes/gpui-kit/gruvbox.json")),
    ("harper", include_str!("../themes/gpui-kit/harper.json")),
    ("hybrid", include_str!("../themes/gpui-kit/hybrid.json")),
    (
        "jellybeans",
        include_str!("../themes/gpui-kit/jellybeans.json"),
    ),
    ("kibble", include_str!("../themes/gpui-kit/kibble.json")),
    (
        "macos-classic",
        include_str!("../themes/gpui-kit/macos-classic.json"),
    ),
    (
        "mellifluous",
        include_str!("../themes/gpui-kit/mellifluous.json"),
    ),
    ("molokai", include_str!("../themes/gpui-kit/molokai.json")),
    (
        "solarized",
        include_str!("../themes/gpui-kit/solarized.json"),
    ),
    (
        "spaceduck",
        include_str!("../themes/gpui-kit/spaceduck.json"),
    ),
    (
        "tokyonight",
        include_str!("../themes/gpui-kit/tokyonight.json"),
    ),
    ("twilight", include_str!("../themes/gpui-kit/twilight.json")),
];
pub const THEME_PREF: &str = "appearance.theme";
pub const FONT_PREF: &str = "appearance.font";
const HEADING_PREF: &str = "appearance.heading-font";
const BODY_PREF: &str = "appearance.body-font";
const MONO_PREF: &str = "appearance.mono-font";
pub(crate) const AUTOMATIC_FONT: &str = "Automatic (theme default)";

/// The interface base at the default body size; control tokens are pixels at it.
const FONT_SIZE: f32 = density::REFERENCE_REM;
const SYSTEM_FONT: &str = ".SystemUIFont";
/// GPUI Kit's existing Markdown heading hierarchy, in pixels.
const RICH_HEADING_SIZES: [f32; 6] = [28., 21., 17.5, 15.75, 14., 14.];

fn saved_theme_or_default<'a>(
    saved: Option<&'a str>,
    names: &[String],
    default: &'a str,
) -> (&'a str, bool) {
    let had_saved = saved.is_some();
    match saved.filter(|saved| names.iter().any(|name| name == saved)) {
        Some(saved) => (saved, false),
        None => (default, had_saved),
    }
}

/// Exact live values, not a reconstruction from possibly changed files.
#[derive(Clone)]
pub(crate) struct AppearanceSnapshot {
    theme: Theme,
    density: Density,
    typography: Typography,
    tones: ThemeTones,
    id: ThemeId,
    preview: bool,
}

impl AppearanceSnapshot {
    pub fn capture(cx: &App) -> Self {
        Self {
            theme: Theme::global(cx).clone(),
            density: cx.try_global::<Density>().cloned().unwrap_or_default(),
            typography: cx.try_global::<Typography>().cloned().unwrap_or_default(),
            tones: cx.try_global::<ThemeTones>().cloned().unwrap_or_default(),
            id: cx.try_global::<ThemeId>().cloned().unwrap_or_default(),
            preview: cx
                .try_global::<DensityPreview>()
                .is_some_and(|preview| preview.0),
        }
    }

    pub fn restore(&self, cx: &mut App) {
        cx.set_global(self.density.clone());
        cx.set_global(self.typography.clone());
        cx.set_global(self.tones.clone());
        cx.set_global(self.id.clone());
        cx.set_global(DensityPreview(self.preview));
        // A mode change reloads that mode's registered config over the edit,
        // dropping adjustments such as the switch track, so switch mode first.
        Theme::update(cx, |theme| theme.mode = self.theme.mode);
        Theme::update(cx, |theme| *theme = self.theme.clone());
        sync_rich_text(cx);
    }
}

pub(crate) fn document(name: &str, cx: &App) -> Result<ThemeDocument, String> {
    cx.global::<ThemeChoices>()
        .documents
        .get(name)
        .cloned()
        .ok_or_else(|| format!("Unknown theme: {name}"))
}

/// Applies a validated document without changing the catalogue or preferences.
pub(crate) fn apply_document(
    document: &ThemeDocument,
    choices: &FontChoices,
    cx: &mut App,
) -> Result<Vec<String>, String> {
    let config = std::rc::Rc::new(document.config()?);
    let metadata = document.metadata()?;
    let mut resolved = resolve_roles(&config, &metadata.fonts, choices, cx);
    let group_font = fonts::resolve_group_heading(
        &metadata.group_heading.font,
        &resolved.heading,
        &font_names(cx),
        &mut resolved.warnings,
    );
    let tones = metadata
        .tones
        .iter()
        .map(|(name, value)| {
            try_parse_color(value)
                .map(|color| (name.clone(), color))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    cx.set_global(Typography {
        heading: resolved.heading.into(),
        group_heading: GroupHeading {
            font: group_font.into(),
            size: metadata.group_heading.size,
            bold: metadata.group_heading.bold,
            italic: metadata.group_heading.italic,
        },
    });
    cx.set_global(ThemeTones(tones));
    cx.set_global(Density::from_json(&metadata.density.to_string()));
    Theme::update(cx, |theme| {
        theme.apply_config(&config);
        subspace_dashboard::toggle::track::apply(theme);
        theme.font_family = resolved.body.into();
        theme.mono_font_family = resolved.mono.into();
    });
    let id = bundled_theme_id(Theme::global(cx).theme_name(), product_themes(cx));
    cx.set_global(ThemeId(id.to_owned()));
    sync_font_sizes(cx);
    Ok(resolved.warnings)
}

/// Apply role sizes after any live density change, including saved overrides.
pub(crate) fn sync_font_sizes(cx: &mut App) {
    let tokens = subspace_dashboard::Tokens::current(cx);
    Theme::update(cx, |theme| {
        theme.font_size = px(FONT_SIZE) * (f32::from(tokens.row_font) / BODY_FONT_DEFAULT);
        theme.mono_font_size = tokens.mono_font;
    });
    sync_rich_text(cx);
}

fn sync_rich_text(cx: &mut App) {
    let heading = subspace_dashboard::typography::heading_font(cx);
    let tokens = subspace_dashboard::Tokens::current(cx);
    let style = subspace_dashboard::rich::text_view_style(cx).with_heading(move |level| {
        gpui_kit::StyleRefinement::default()
            .font_family(heading.clone())
            .text_size(tokens.heading_size(rich_heading_size(level)))
    });
    gpui_kit::base::TextViewDefaults::global(cx)
        .with_style(style)
        .install(cx);
}

fn rich_heading_size(level: u8) -> f32 {
    RICH_HEADING_SIZES
        .get(usize::from(level).saturating_sub(1))
        .copied()
        .unwrap_or(FONT_SIZE)
}

pub(crate) fn effective_fonts(choices: &FontChoices, defaults: &FontChoices) -> FontChoices {
    let choose = |choice: &str, default: &str| {
        if choice.is_empty() {
            default.to_owned()
        } else {
            choice.to_owned()
        }
    };
    FontChoices {
        heading: choose(&choices.heading, &defaults.heading),
        body: choose(&choices.body, &defaults.body),
        mono: choose(&choices.mono, &defaults.mono),
    }
}

fn config_fonts(config: &gpui_kit::component::ThemeConfig, metadata: &FontChoices) -> FontChoices {
    let body = config
        .font_family
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    let mono = config
        .mono_font_family
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    effective_fonts(
        metadata,
        &FontChoices {
            heading: String::new(),
            body,
            mono,
        },
    )
}

fn resolve_roles(
    config: &gpui_kit::component::ThemeConfig,
    defaults: &FontChoices,
    choices: &FontChoices,
    cx: &App,
) -> fonts::ResolvedFonts {
    let defaults = config_fonts(config, defaults);
    let requested = effective_fonts(choices, &defaults);
    fonts::resolve(
        &requested.heading,
        &requested.body,
        &requested.mono,
        &font_names(cx),
        &crate::bundled_fonts::bundled_only(cx),
    )
}

pub(crate) fn font_warnings(
    name: &str,
    choices: &FontChoices,
    cx: &App,
) -> Result<Vec<String>, String> {
    let document = document(name, cx)?;
    Ok(resolve_roles(
        &document.config()?,
        &document.metadata()?.fonts,
        choices,
        cx,
    )
    .warnings)
}

/// Register the bundled and user themes, then restore saved appearance choices.
/// Invalid files and unavailable saved choices are returned as non-fatal warnings.
pub fn initialize(cx: &mut App) -> Vec<String> {
    let mut warnings = load_registered_themes(cx);
    let names = theme_names(cx);
    let saved_theme = prefs::get(cx, THEME_PREF);
    let default = default_theme(cx);
    let (theme, theme_fell_back) = saved_theme_or_default(saved_theme.as_deref(), &names, &default);
    if theme_fell_back {
        warnings.push(format!("saved theme is unavailable; using {default}"));
    }

    let result =
        document(theme, cx).and_then(|document| apply_document(&document, &selected_fonts(cx), cx));
    match result {
        Ok(font_warnings) => warnings.extend(font_warnings),
        Err(error) => warnings.push(error),
    }
    if let Some(saved) = prefs::get(cx, density::STATE_KEY) {
        cx.set_global(Density::from_json(&saved));
    }
    sync_font_sizes(cx);
    warnings
}

/// Startup: registers the app's embedded fonts, then [`initialize`], so a
/// theme that names them resolves them without a missing-font notice.
pub(crate) fn start(fonts: &'static [crate::BundledFont], cx: &mut App) -> Vec<String> {
    let notice = crate::bundled_fonts::register(fonts, cx);
    notice.into_iter().chain(initialize(cx)).collect()
}

pub fn theme_names(cx: &App) -> Vec<String> {
    let Some(choices) = cx.try_global::<ThemeChoices>() else {
        return Vec::new();
    };
    let mut names = choices.documents.keys().cloned().collect::<Vec<_>>();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

/// The installed families, with the fonts the app registered at startup,
/// which count as installed even where the text system does not list them.
pub fn font_names(cx: &App) -> Vec<String> {
    let platform = cx.text_system().all_font_names();
    installed_families(platform, crate::bundled_fonts::registered(cx))
}

/// The platform's families and the registered ones, sorted without case.
fn installed_families(mut names: Vec<String>, registered: Vec<String>) -> Vec<String> {
    names.extend(registered);
    // GPUI maps this virtual family to the platform UI font; it is never listed.
    names.push(SYSTEM_FONT.to_owned());
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}

pub(crate) fn font_choices(cx: &App) -> Vec<String> {
    let mut choices: Vec<_> = std::iter::once(AUTOMATIC_FONT.to_owned())
        .chain(font_names(cx))
        .collect();
    let saved = selected_fonts(cx);
    for font in [saved.heading, saved.body, saved.mono] {
        if !font.is_empty() && !choices.contains(&font) {
            choices.push(font);
        }
    }
    if let Ok(document) = document(&selected_theme(cx), cx)
        && let Ok(metadata) = document.metadata()
        && !metadata.group_heading.font.is_empty()
        && !choices.contains(&metadata.group_heading.font)
    {
        choices.push(metadata.group_heading.font);
    }
    choices
}

pub fn selected_theme(cx: &App) -> String {
    let saved = prefs::get(cx, THEME_PREF);
    let names = theme_names(cx);
    saved_theme_or_default(saved.as_deref(), &names, &default_theme(cx))
        .0
        .to_owned()
}

pub fn selected_fonts(cx: &App) -> FontChoices {
    let legacy = prefs::get(cx, FONT_PREF);
    font_preferences(
        prefs::get(cx, HEADING_PREF),
        prefs::get(cx, BODY_PREF),
        prefs::get(cx, MONO_PREF),
        legacy,
    )
}

fn font_preferences(
    heading: Option<String>,
    body: Option<String>,
    mono: Option<String>,
    legacy: Option<String>,
) -> FontChoices {
    FontChoices {
        heading: heading.or_else(|| legacy.clone()).unwrap_or_default(),
        body: body.or_else(|| legacy.clone()).unwrap_or_default(),
        mono: mono.or(legacy).unwrap_or_default(),
    }
}

/// Apply a selection immediately without changing saved preferences.
///
/// Callers can use this for previews and restore the saved choices with another
/// preview or `initialize`; no preference file is read or written here.
pub fn preview_selection(
    theme: &str,
    fonts: &FontChoices,
    cx: &mut App,
) -> Result<Vec<String>, String> {
    document::validate_fonts(fonts)?;
    let density =
        (Theme::global(cx).theme_name().as_ref() == theme).then(|| cx.global::<Density>().clone());
    let warnings = apply_document(&document(theme, cx)?, fonts, cx)?;
    if let Some(density) = density {
        cx.set_global(density);
    }
    sync_font_sizes(cx);
    Ok(warnings)
}

/// Persist both choices first, then apply them to the live application.
pub fn save_selection(theme: &str, fonts: &FontChoices, cx: &mut App) -> Result<(), String> {
    let before = AppearanceSnapshot::capture(cx);
    let density = cx.global::<Density>().clone();
    preview_selection(theme, fonts, cx)?;
    cx.set_global(density);
    sync_font_sizes(cx);
    if let Err(error) = persist_selection(theme, fonts, cx) {
        before.restore(cx);
        return Err(error);
    }
    Ok(())
}

fn persist_selection(theme: &str, fonts: &FontChoices, cx: &mut App) -> Result<(), String> {
    let density = cx.global::<Density>().to_json();
    let changes = [
        (THEME_PREF, Some(theme)),
        (HEADING_PREF, Some(fonts.heading.as_str())),
        (BODY_PREF, Some(fonts.body.as_str())),
        (MONO_PREF, Some(fonts.mono.as_str())),
        (FONT_PREF, None),
        (density::STATE_KEY, Some(density.as_str())),
    ];
    let before = changes.map(|(key, _)| (key, prefs::get(cx, key)));
    for (key, value) in changes {
        write_preference(key, value, cx);
    }
    if let Err(error) = prefs::save(cx) {
        for (key, value) in before {
            write_preference(key, value.as_deref(), cx);
        }
        return Err(format!("Could not save appearance: {error}"));
    }
    Ok(())
}

fn write_preference(key: &str, value: Option<&str>, cx: &mut App) {
    match value {
        Some(value) => prefs::set(cx, key, value),
        None => prefs::remove(cx, key),
    }
}

pub(crate) fn theme_directory(cx: &App) -> Result<std::path::PathBuf, String> {
    crate::product::gui_config_file(cx)
        .parent()
        .map(|path| path.join("themes"))
        .ok_or_else(|| "Product configuration has no parent directory".into())
}

pub(crate) fn reload(cx: &mut App) -> Vec<String> {
    load_registered_themes(cx)
}

#[cfg(test)]
mod tests {
    use super::loader::validate_custom_theme;
    use super::{
        GPUI_KIT_THEMES, SUBSPACE_THEMES, THEME_JSON, THEME_NAME, font_preferences,
        saved_theme_or_default,
    };
    use gpui_kit::component::ThemeSet;
    use subspace_dashboard::tokens::{glass, hex};

    /// One bundled variant's control tokens, as the theme applies them.
    fn controls(content: &str, name: &str) -> subspace_dashboard::tokens::Controls {
        let document = super::ThemeDocument::parse(content)
            .unwrap()
            .variant(name)
            .unwrap();
        let density = document.metadata().unwrap().density.to_string();
        subspace_dashboard::tokens::Controls::from_density(
            &subspace_dashboard::density::Density::from_json(&density),
        )
    }

    #[test]
    fn macos_themes_set_liquid_glass_controls_and_others_keep_the_defaults() {
        use gpui_kit::px;
        let defaults = subspace_dashboard::tokens::Tokens::one_dark().controls;
        for (content, name) in [
            (super::CUPERTINO_JSON, "Cupertino Light"),
            (super::CUPERTINO_JSON, "Cupertino Dark"),
            (THEME_JSON, THEME_NAME),
        ] {
            let macos = controls(content, name);
            assert!(macos.liquid_glass, "{name}");
            // macOS 26: medium (regular) 24, small 20, mini 16 point controls.
            assert_eq!(macos.toolbar_button.height, px(24.), "{name}");
            assert_eq!(macos.toolbar_button.radius, px(12.), "{name}: a capsule");
            assert_eq!(macos.dialog_button.height, px(24.), "{name}");
            assert_eq!(macos.dialog_button.font, px(13.), "{name}");
            assert_eq!(macos.action_button.height, px(20.), "{name}");
            assert_eq!(macos.action_button.font, px(11.), "{name}");
            assert_eq!(macos.compact_button.height, px(16.), "{name}");
            assert!(
                macos.compact_button.font >= px(10.),
                "{name}: the HIG minimum"
            );
            assert_eq!(macos.dialog_pad, px(20.), "{name}");
        }
        let solarized = GPUI_KIT_THEMES
            .iter()
            .find(|(id, _)| *id == "solarized")
            .unwrap()
            .1;
        for name in ["Solarized Light", "Solarized Dark"] {
            assert_eq!(controls(solarized, name), defaults, "{name} keeps its look");
        }
    }

    /// Twilight Two is the workspace mock-ups' look, sizes included: text
    /// 16 px in from each panel edge, inset 32 px rows, 34 px strips and a
    /// 36 px toolbar of 24 px controls with 6 px corners.
    #[test]
    fn twilight_two_carries_the_mock_up_sizes() {
        use gpui_kit::px;
        let document = super::ThemeDocument::parse(super::TWILIGHT_TWO_JSON)
            .unwrap()
            .variant("Twilight Two")
            .unwrap();
        let density = document.metadata().unwrap().density.to_string();
        let tokens = subspace_dashboard::Tokens::with_density(
            &subspace_dashboard::density::Density::from_json(&density),
        );
        assert_eq!(tokens.pad_x + tokens.row_inset, px(16.), "the text edge");
        assert!(tokens.inset_rows());
        assert_eq!(tokens.row_height, px(32.));
        assert_eq!(tokens.strip_height, px(34.));
        assert_eq!(tokens.toolbar_height, px(36.));
        assert_eq!(tokens.chrome_height, px(38.));
        assert_eq!(tokens.control_radius, px(6.));
        assert_eq!(tokens.heading_font, px(16.));
        assert_eq!(tokens.row_font, px(14.));
        assert_eq!(tokens.mono_font, px(12.5));
        let toolbar = tokens.controls.toolbar_button;
        assert_eq!(toolbar.height, px(24.));
        assert_eq!(toolbar.radius, px(6.));
        assert_eq!(toolbar.font, px(13.5));
        assert_eq!(tokens.controls.nav_height, px(26.));
        assert!(!tokens.controls.liquid_glass);
    }

    /// Twilight Two's code and Markdown colours, Markdown URLs and type names
    /// among them, read as normal text (WCAG 1.4.3, 4.5:1) on every surface
    /// they are drawn on: the page, the side panels and popovers, the editor
    /// and its current line.
    #[test]
    fn twilight_two_syntax_colours_meet_normal_text_contrast() {
        let document: serde_json::Value = serde_json::from_str(super::TWILIGHT_TWO_JSON).unwrap();
        let theme = &document["themes"][0];
        let surfaces = [
            &theme["colors"]["background"],
            &theme["colors"]["sidebar.background"],
            &theme["colors"]["popover.background"],
            &theme["highlight"]["editor.background"],
            &theme["highlight"]["editor.active_line.background"],
        ]
        .map(|surface| surface.as_str().unwrap());
        let syntax = theme["highlight"]["syntax"].as_object().unwrap();
        for key in ["link_uri", "link_text", "type", "title"] {
            assert!(syntax.contains_key(key), "{key}");
        }
        for (key, style) in syntax {
            let colour = style["color"].as_str().unwrap();
            for surface in surfaces {
                let ratio = contrast(colour, surface);
                assert!(ratio >= 4.5, "{key} {colour} on {surface}: {ratio:.2}");
            }
        }
    }

    /// WCAG 2.2 non-text contrast (1.4.11): an off switch must stay visible
    /// on every surface a form can sit on, and its thumb on its track.
    #[test]
    fn every_subspace_theme_draws_an_off_switch_with_non_text_contrast() {
        for (_, document) in SUBSPACE_THEMES {
            let document: serde_json::Value = serde_json::from_str(document).unwrap();
            for theme in document["themes"].as_array().unwrap() {
                let colors = &theme["colors"];
                let colour = |key: &str| colors[key].as_str().unwrap().to_string();
                let track = colour("switch.background");
                for surface in ["background", "popover.background", "secondary.background"] {
                    let ratio = contrast(&track, &colour(surface));
                    assert!(
                        ratio >= 3.0,
                        "{} off switch on {surface}: {ratio}",
                        theme["name"]
                    );
                }
                let thumb = contrast(&colour("switch.thumb.background"), &track);
                assert!(thumb >= 3.0, "{} thumb on track: {thumb}", theme["name"]);
            }
        }
    }

    /// The current board's glass capsule must read as a distinct surface on
    /// the title bar and keep its label legible (WCAG 1.4.3).
    #[test]
    fn every_subspace_theme_keeps_glass_quiet_visible_and_legible() {
        use subspace_dashboard::contrast::{ratio, ratio_on};
        for (_, document) in SUBSPACE_THEMES {
            let document: serde_json::Value = serde_json::from_str(document).unwrap();
            for theme in document["themes"].as_array().unwrap() {
                let name = &theme["name"];
                let bar = theme["colors"]["title_bar.background"].as_str().unwrap();
                let label = theme["colors"]["caret"].as_str().unwrap();
                let glass = theme["subspace"]["tones"]["glass"].as_str().unwrap();
                let surface = ratio(glass, bar).unwrap();
                assert!(
                    (1.05..2.0).contains(&surface),
                    "{name} glass {surface} on its bar"
                );
                let text = ratio_on(label, glass, bar).unwrap();
                assert!(text >= 4.5, "{name} label {text} on glass");
            }
        }
    }

    /// A flat button's glyph and text stay legible (WCAG 1.4.3) on its hover
    /// and pressed fills, and a latched button's accent-tinted glyph and text
    /// on its fills, on the title bar and on the page.
    #[test]
    fn every_subspace_theme_keeps_flat_button_glyphs_legible_on_their_fills() {
        use gpui_kit::Hsla;
        use subspace_dashboard::icon_button::paint;
        use subspace_dashboard::tokens::ToolbarLook;
        for (_, document) in SUBSPACE_THEMES {
            let document: serde_json::Value = serde_json::from_str(document).unwrap();
            for theme in document["themes"].as_array().unwrap() {
                let tokens = flat_tokens(theme);
                let off = paint(ToolbarLook::FLAT, false, &tokens, Hsla::default());
                let on = paint(ToolbarLook::FLAT, true, &tokens, Hsla::default());
                let strong = hex(tokens.fg_strong);
                for surface in ["title_bar.background", "background"] {
                    let base = theme["colors"][surface].as_str().unwrap();
                    let label = format!("{} on {surface}", theme["name"]);
                    assert_legible(&label, &strong, &[off.hover, off.pressed], base);
                    assert_legible(
                        &label,
                        &hex(on.glyph),
                        &[on.rest, on.hover, on.pressed],
                        base,
                    );
                }
            }
        }
    }

    /// One bundled variant's colours with the default flat button tones.
    fn flat_tokens(theme: &serde_json::Value) -> subspace_dashboard::Tokens {
        use gpui_kit::component::try_parse_color;
        use gpui_kit::{Hsla, rgba};
        use subspace_dashboard::tokens::flat;
        let colour = |key: &str| try_parse_color(theme["colors"][key].as_str().unwrap()).unwrap();
        let (hover, on) = if theme["mode"] == "dark" {
            (flat::DARK_HOVER, flat::DARK_ON)
        } else {
            (flat::LIGHT_HOVER, flat::LIGHT_ON)
        };
        let accent = colour("primary.background");
        subspace_dashboard::Tokens {
            accent,
            fg_strong: colour("caret"),
            button_hover: Hsla::from(rgba(hover)),
            button_on: accent.opacity(on),
            ..subspace_dashboard::Tokens::one_dark()
        }
    }

    fn assert_legible(label: &str, text: &str, fills: &[gpui_kit::Hsla], base: &str) {
        for fill in fills {
            let ratio = subspace_dashboard::contrast::ratio_on(text, &hex(*fill), base).unwrap();
            assert!(ratio >= 4.5, "{label}: {ratio} on {fill:?}");
        }
    }

    fn hex(color: gpui_kit::Hsla) -> String {
        let color = color.to_rgb();
        let byte = |channel: f32| (channel * 255.).round() as u8;
        let [r, g, b, a] = [color.r, color.g, color.b, color.a].map(byte);
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }

    fn contrast(foreground: &str, background: &str) -> f64 {
        let foreground = luminance(parse_rgb(foreground));
        let background = luminance(parse_rgb(background));
        (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
    }

    fn parse_rgb(value: &str) -> [f64; 3] {
        let value = value.strip_prefix('#').unwrap();
        [0, 2, 4].map(|start| {
            f64::from(u8::from_str_radix(&value[start..start + 2], 16).unwrap()) / 255.0
        })
    }

    fn luminance(rgb: [f64; 3]) -> f64 {
        let [red, green, blue] = rgb.map(|channel| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    }

    #[test]
    fn the_theme_parses_and_matches_the_dashboard_tokens() {
        assert_eq!(
            THEME_JSON.as_bytes(),
            include_bytes!("../../../board-kit/dashboard/examples/themes/one-dark.json")
        );
        let set: ThemeSet = serde_json::from_str(THEME_JSON).unwrap();
        assert_eq!(set.themes.len(), 1);
        assert_eq!(set.themes[0].name.as_ref(), THEME_NAME);
        assert!(set.themes[0].mode.is_dark());
        assert!(set.themes[0].font_family.is_none());
        let json: serde_json::Value = serde_json::from_str(THEME_JSON).unwrap();
        let colors = &json["themes"][0]["colors"];
        for (key, value) in [
            ("background", hex::BG),
            ("foreground", hex::FG),
            ("border", hex::LINE),
            ("muted.foreground", hex::MUTED),
            ("muted.background", hex::PANE),
            ("accent.background", hex::SELECT),
            ("ring", hex::ACCENT),
            ("list.hover.background", hex::HAIR),
            ("danger.background", hex::RED),
            ("warning.background", hex::YELLOW),
            ("success.background", hex::GREEN),
            ("info.background", hex::CYAN),
            ("base.blue", hex::BLUE),
            ("base.magenta", hex::MAGENTA),
        ] {
            assert_eq!(colors[key], format!("#{value:06x}"), "{key}");
        }
        let tones = &json["themes"][0]["subspace"]["tones"];
        for (key, value) in [
            ("glass", glass::DARK_FILL),
            ("glass-rim", glass::DARK_RIM),
            ("glass-shadow", glass::DARK_SHADOW),
        ] {
            assert_eq!(tones[key], format!("#{value:08x}"), "{key}");
        }
        let highlight = &json["themes"][0]["highlight"];
        assert_eq!(highlight["syntax"]["comment"]["color"], "#7f848e");
        assert_eq!(highlight["syntax"]["number"]["color"], "#d19a66");
        assert_eq!(
            highlight["editor.background"],
            format!("#{:06x}", hex::PANE)
        );
        assert_eq!(highlight["editor.foreground"], format!("#{:06x}", hex::FG));
        assert_eq!(
            highlight["syntax"]["string"]["color"],
            format!("#{:06x}", hex::GREEN)
        );
    }

    #[test]
    fn every_upstream_theme_is_embedded_and_parser_safe() {
        let mut variants = std::collections::HashSet::new();
        let expected = [
            "Adventure",
            "Alduin",
            "Asciinema",
            "Aurora",
            "Ayu",
            "Catppuccin",
            "Everforest",
            "Fahrenheit",
            "Flexoki",
            "Gruvbox",
            "Harper",
            "Hybrid",
            "Jellybeans",
            "Kibble",
            "macOS Classic",
            "Mellifluous",
            "Molokai",
            "Solarized",
            "Spaceduck",
            "Tokyo",
            "Twilight",
        ];
        assert_eq!(GPUI_KIT_THEMES.len(), expected.len());
        for ((file_name, content), expected_name) in GPUI_KIT_THEMES.iter().zip(expected) {
            let set: ThemeSet = serde_json::from_str(content)
                .unwrap_or_else(|error| panic!("{file_name} must parse: {error}"));
            assert_eq!(set.name, expected_name, "{file_name}");
            assert!(!set.themes.is_empty(), "{file_name}");
            for variant in &set.themes {
                assert_ne!(variant.name.as_ref(), THEME_NAME);
                assert!(
                    variants.insert(variant.name.to_string()),
                    "duplicate theme {}",
                    variant.name
                );
            }
        }
        assert_eq!(
            variants.len(),
            36,
            "all upstream theme variants must be available"
        );
    }

    #[test]
    fn custom_themes_cannot_replace_the_bundled_name() {
        assert!(validate_custom_theme(THEME_JSON, &[]).is_err());
        assert!(validate_custom_theme(&THEME_JSON.replace(THEME_NAME, "Custom"), &[]).is_ok());
    }

    #[test]
    fn every_theme_declares_an_opaque_media_stage_in_its_palette() {
        for (_, source) in GPUI_KIT_THEMES.iter().chain(SUBSPACE_THEMES.iter()) {
            let json: serde_json::Value = serde_json::from_str(source).unwrap();
            for variant in json["themes"].as_array().unwrap() {
                let stage = variant["subspace"]["tones"]["media-stage"]
                    .as_str()
                    .unwrap();
                assert_eq!(stage.len(), 7, "{}: opaque stage", variant["name"]);
                assert!(stage.starts_with('#'));
                assert!(
                    contrast(variant["colors"]["foreground"].as_str().unwrap(), stage) >= 3.,
                    "{}",
                    variant["name"]
                );
            }
        }
        let one_dark: serde_json::Value = serde_json::from_str(THEME_JSON).unwrap();
        assert_eq!(
            one_dark["themes"][0]["subspace"]["tones"]["media-stage"],
            format!("#{:06x}", hex::BG)
        );
    }

    #[test]
    fn unavailable_saved_choices_fall_back() {
        let names = vec![THEME_NAME.to_owned()];
        assert_eq!(
            saved_theme_or_default(Some("Missing"), &names, THEME_NAME),
            (THEME_NAME, true)
        );
        assert_eq!(
            saved_theme_or_default(None, &names, THEME_NAME),
            (THEME_NAME, false)
        );
        assert_eq!(
            saved_theme_or_default(None, &names, "Custom default"),
            ("Custom default", false)
        );
    }

    #[test]
    fn independent_preferences_override_only_their_legacy_role() {
        let fonts = font_preferences(
            Some("Inter".into()),
            Some("".into()),
            None,
            Some("Menlo".into()),
        );
        assert_eq!(fonts.heading, "Inter");
        assert_eq!(fonts.body, "");
        assert_eq!(fonts.mono, "Menlo");
    }
}
